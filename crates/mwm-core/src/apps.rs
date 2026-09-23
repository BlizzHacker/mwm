//! Installed programs, uninstall, and Revo-style leftover hunting.
//!
//! Leftover removal is deliberately reversible: folders and files go to the
//! Recycle Bin / Trash, registry keys are exported to a `.reg` backup first.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsutil;
use crate::util::{self, norm};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: String,
    pub size_bytes: u64,
    pub install_location: String,
    pub uninstall_cmd: String,
    pub quiet_uninstall_cmd: String,
    pub icon: String,
    /// win32-machine | win32-user | store | dpkg | rpm | flatpak | snap | macos
    pub source: String,
    pub reg_key: String,
    pub can_uninstall: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leftover {
    /// dir | file | regkey
    pub kind: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub ok: bool,
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveReport {
    pub removed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub backup: Option<String>,
}

/// Words that are never a program's own folder, however the name normalizes.
const NEVER: &[&str] = &[
    "microsoft", "windows", "google", "apple", "intel", "amd", "nvidia", "nvidiacorporation", "mozilla", "common",
    "commonfiles", "programdata", "packages", "temp", "cache", "system", "system32", "users", "public", "programs",
    "startup", "microsoftcorporation", "windowsapps", "classes", "policies", "wow6432node", "software", "local",
    "roaming", "locallow", "appdata", "desktop", "documents", "downloads", "default", "updater", "update", "app",
    "application", "applications", "tools", "games", "data", "config", "python", "java", "node", "nodejs",
];

fn strip_versions(name: &str) -> String {
    let mut s = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => s.push(ch),
            _ => {}
        }
    }
    s.split_whitespace()
        .filter(|w| {
            let lw = w.to_lowercase();
            let versiony = w.chars().any(|c| c.is_ascii_digit()) && (w.contains('.') || w.chars().all(|c| c.is_ascii_digit()));
            !(versiony
                || matches!(lw.as_str(), "x64" | "x86" | "64-bit" | "32-bit" | "amd64" | "arm64" | "-" | "version" | "v"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn publisher_key(p: &str) -> String {
    let mut s = p.to_string();
    for suffix in [", Inc.", " Inc.", " Inc", ", LLC", " LLC", " Ltd.", " Ltd", " Corporation", " Corp.", " GmbH", " Co.", " s.r.o.", " B.V.", " AG"] {
        if let Some(i) = s.find(suffix) {
            s.truncate(i);
        }
    }
    norm(&s)
}

/// Names a leftover folder/key could plausibly carry for this app.
fn name_keys(app: &App) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for k in [norm(&strip_versions(&app.name)), norm(&app.name)] {
        if k.len() >= 4 && !NEVER.contains(&k.as_str()) && !keys.contains(&k) {
            keys.push(k);
        }
    }
    if !app.install_location.is_empty() {
        if let Some(base) = Path::new(app.install_location.trim_matches('"')).file_name() {
            let k = norm(&base.to_string_lossy());
            if k.len() >= 4 && !NEVER.contains(&k.as_str()) && !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    keys
}

fn others_keys(app: &App, all: &[App]) -> (HashSet<String>, Vec<PathBuf>) {
    let mut names = HashSet::new();
    let mut locations = Vec::new();
    for o in all.iter().filter(|o| o.id != app.id) {
        names.insert(norm(&strip_versions(&o.name)));
        names.insert(norm(&o.name));
        let loc = o.install_location.trim().trim_matches('"');
        if !loc.is_empty() {
            locations.push(PathBuf::from(loc));
        }
    }
    (names, locations)
}

fn path_shared(p: &Path, other_locations: &[PathBuf]) -> bool {
    let lp = p.to_string_lossy().to_lowercase();
    other_locations.iter().any(|o| {
        let lo = o.to_string_lossy().to_lowercase();
        lo == lp || lo.starts_with(&format!("{lp}{}", std::path::MAIN_SEPARATOR))
    })
}

fn trimmed_lower(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase()
}

fn same_path(a: &Path, b: &Path) -> bool {
    trimmed_lower(a) == trimmed_lower(b)
}

fn is_ancestor_or_same(anc: &Path, p: &Path) -> bool {
    let a = trimmed_lower(anc);
    let q = trimmed_lower(p);
    q == a || q.starts_with(&format!("{a}\\")) || q.starts_with(&format!("{a}/"))
}

/// Look inside `roots` for folders (and `publisher\name` folders) that belong
/// to `app` and to no other installed program.
fn scan_dirs(app: &App, all: &[App], roots: &[PathBuf], extra_files: &[(PathBuf, &[&str])]) -> Vec<Leftover> {
    let keys = name_keys(app);
    let pubk = publisher_key(&app.publisher);
    let (other_names, other_locs) = others_keys(app, all);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push_dir = |p: PathBuf, out: &mut Vec<Leftover>| {
        if path_shared(&p, &other_locs) || !seen.insert(p.to_string_lossy().to_lowercase()) {
            return;
        }
        let t = fsutil::tree_size(&p);
        out.push(Leftover { kind: "dir".into(), path: p.to_string_lossy().into(), bytes: t.bytes });
    };

    let loc = app.install_location.trim().trim_matches('"');
    if !loc.is_empty() {
        let p = PathBuf::from(loc);
        let pn = norm(&p.file_name().unwrap_or_default().to_string_lossy());
        let own = norm(&strip_versions(&app.name));
        let named_after_app = own.len() >= 4 && (pn == own || pn.contains(&own) || pn == norm(&app.name));
        let under_program_root = p.parent().map(|pp| roots.iter().any(|r| same_path(r, pp))).unwrap_or(false);
        // Never offer a folder that holds a program root or the user profile
        // (some installers register C:\Users\<you> as their location).
        let home = util::home();
        let holds_something = roots.iter().chain(std::iter::once(&home)).any(|r| is_ancestor_or_same(&p, r));
        if p.is_dir()
            && p.components().count() > 2
            && (named_after_app || under_program_root)
            && !holds_something
            && !NEVER.contains(&pn.as_str())
        {
            push_dir(p, &mut out);
        }
    }

    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let n = norm(&e.file_name().to_string_lossy());
            if keys.contains(&n) {
                if !other_names.contains(&n) {
                    push_dir(e.path(), &mut out);
                }
            } else if !pubk.is_empty() && pubk.len() >= 3 && n == pubk && !NEVER.contains(&n.as_str()) {
                if let Ok(inner) = std::fs::read_dir(e.path()) {
                    for ie in inner.flatten() {
                        let inn = norm(&ie.file_name().to_string_lossy());
                        if keys.contains(&inn) && !other_names.contains(&inn) && ie.path().is_dir() {
                            push_dir(ie.path(), &mut out);
                        }
                    }
                }
            }
        }
    }

    for (dir, exts) in extra_files {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let stem = norm(&p.file_stem().unwrap_or_default().to_string_lossy());
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            if p.is_file() && exts.contains(&ext.as_str()) && keys.contains(&stem) {
                let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(Leftover { kind: "file".into(), path: p.to_string_lossy().into(), bytes });
            }
        }
    }
    out
}

pub fn remove_files_to_trash(items: &[Leftover]) -> RemoveReport {
    let mut rep = RemoveReport { removed: vec![], failed: vec![], backup: None };
    for it in items.iter().filter(|i| i.kind != "regkey") {
        match trash::delete(&it.path) {
            Ok(()) => rep.removed.push(it.path.clone()),
            Err(e) => rep.failed.push((it.path.clone(), e.to_string())),
        }
    }
    rep
}

// ============================================================== Windows ====

#[cfg(windows)]
mod platform {
    use super::*;
    use winreg::enums::*;
    use winreg::{RegKey, HKEY};

    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const UNINSTALL32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

    fn read_hive(root: HKEY, root_name: &str, path: &str, source: &str, out: &mut Vec<App>) {
        let Ok(base) = RegKey::predef(root).open_subkey_with_flags(path, KEY_READ) else { return };
        for sub in base.enum_keys().flatten() {
            let Ok(k) = base.open_subkey_with_flags(&sub, KEY_READ) else { continue };
            let get = |n: &str| k.get_value::<String, _>(n).unwrap_or_default().trim().to_string();
            let name = get("DisplayName");
            if name.is_empty() {
                continue;
            }
            let sysc: u32 = k.get_value("SystemComponent").unwrap_or(0);
            let release = get("ReleaseType");
            if sysc == 1 || !get("ParentKeyName").is_empty() || matches!(release.as_str(), "Update" | "Hotfix" | "Security Update") {
                continue;
            }
            let uninstall = get("UninstallString");
            let est_kb: u32 = k.get_value("EstimatedSize").unwrap_or(0);
            let date = get("InstallDate");
            let date = if date.len() == 8 && date.chars().all(|c| c.is_ascii_digit()) {
                format!("{}-{}-{}", &date[0..4], &date[4..6], &date[6..8])
            } else {
                date
            };
            out.push(App {
                id: format!("{source}:{}", sub),
                name,
                version: get("DisplayVersion"),
                publisher: get("Publisher"),
                install_date: date,
                size_bytes: est_kb as u64 * 1024,
                install_location: get("InstallLocation"),
                can_uninstall: !uninstall.is_empty(),
                uninstall_cmd: uninstall,
                quiet_uninstall_cmd: get("QuietUninstallString"),
                icon: get("DisplayIcon"),
                source: source.into(),
                reg_key: format!(r"{root_name}\{path}\{sub}"),
            });
        }
    }

    fn manifest_display_name(dir: &str) -> Option<String> {
        let xml = std::fs::read_to_string(Path::new(dir).join("AppxManifest.xml")).ok()?;
        let props = &xml[xml.find("<Properties>")?..];
        let start = props.find("<DisplayName>")? + "<DisplayName>".len();
        let end = props[start..].find("</DisplayName>")? + start;
        let n = props[start..end].trim();
        (!n.is_empty() && !n.starts_with("ms-resource:")).then(|| n.to_string())
    }

    fn pretty_package_name(n: &str) -> String {
        let tail = n.rsplit('.').next().unwrap_or(n);
        let mut s = String::new();
        let mut prev_lower = false;
        for ch in tail.chars() {
            if ch.is_uppercase() && prev_lower {
                s.push(' ');
            }
            prev_lower = ch.is_lowercase();
            s.push(ch);
        }
        s
    }

    pub fn store_apps() -> Vec<App> {
        let script = "Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.NonRemovable -and $_.SignatureKind -ne 'System' } | Select-Object Name,PackageFullName,Publisher,Version,InstallLocation | ConvertTo-Json -Compress";
        let Ok(rows) = util::powershell_json(script) else { return vec![] };
        rows.iter()
            .map(|v| {
                let name = util::js_str(v, "Name");
                let loc = util::js_str(v, "InstallLocation");
                let display = manifest_display_name(&loc).unwrap_or_else(|| pretty_package_name(&name));
                let publisher = util::js_str(v, "Publisher");
                let publisher = publisher.split(',').next().unwrap_or("").trim_start_matches("CN=").to_string();
                App {
                    id: format!("store:{}", util::js_str(v, "PackageFullName")),
                    name: display,
                    version: util::js_str(v, "Version"),
                    publisher,
                    install_location: loc,
                    source: "store".into(),
                    can_uninstall: true,
                    ..Default::default()
                }
            })
            .collect()
    }

    pub fn list(include_store: bool) -> Vec<App> {
        let mut apps = Vec::new();
        read_hive(HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL, "win32-machine", &mut apps);
        read_hive(HKEY_LOCAL_MACHINE, "HKLM", UNINSTALL32, "win32-machine", &mut apps);
        read_hive(HKEY_CURRENT_USER, "HKCU", UNINSTALL, "win32-user", &mut apps);
        if include_store {
            apps.extend(store_apps());
        }
        // The same product often registers in both views; keep the first.
        let mut seen = HashSet::new();
        apps.retain(|a| seen.insert(format!("{}|{}", a.name.to_lowercase(), a.version)));
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps
    }

    pub fn split_command(s: &str) -> (String, String) {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix('"') {
            if let Some(end) = rest.find('"') {
                return (rest[..end].to_string(), rest[end + 1..].trim().to_string());
            }
        }
        let lower = s.to_ascii_lowercase();
        if let Some(i) = lower.find(".exe") {
            let e = i + 4;
            return (s[..e].to_string(), s[e..].trim().to_string());
        }
        match s.find(' ') {
            Some(i) => (s[..i].to_string(), s[i + 1..].trim().to_string()),
            None => (s.to_string(), String::new()),
        }
    }

    /// ShellExecuteEx so installers that need elevation get a normal UAC prompt.
    pub fn shell_run_wait(file: &str, params: &str, verb: Option<&str>) -> anyhow::Result<i32> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
        use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
        let w = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let file_w = w(file);
        let params_w = w(params);
        let verb_w = verb.map(w);
        unsafe {
            let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
            info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
            info.lpVerb = verb_w.as_ref().map(|v| v.as_ptr()).unwrap_or(std::ptr::null());
            info.lpFile = file_w.as_ptr();
            info.lpParameters = if params.is_empty() { std::ptr::null() } else { params_w.as_ptr() };
            info.nShow = 1; // SW_SHOWNORMAL
            if ShellExecuteExW(&mut info) == 0 {
                anyhow::bail!("could not start {file}: {}", std::io::Error::last_os_error());
            }
            if info.hProcess.is_null() {
                return Ok(0);
            }
            WaitForSingleObject(info.hProcess, INFINITE);
            let mut code = 0u32;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hProcess);
            Ok(code as i32)
        }
    }

    pub fn uninstall(app: &App, quiet: bool) -> Outcome {
        if app.source == "store" {
            let full = app.id.trim_start_matches("store:");
            let script = format!("Remove-AppxPackage -Package '{}' -ErrorAction Stop; 'ok'", full.replace('\'', "''"));
            return match util::run_capture("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", &script]) {
                Ok((code, out)) => Outcome { ok: code == 0 && out.contains("ok"), code, message: out.trim().to_string() },
                Err(e) => Outcome { ok: false, code: -1, message: e.to_string() },
            };
        }
        let cmdline = if quiet && !app.quiet_uninstall_cmd.is_empty() { &app.quiet_uninstall_cmd } else { &app.uninstall_cmd };
        if cmdline.is_empty() {
            return Outcome { ok: false, code: -1, message: "This program has no uninstaller registered.".into() };
        }
        let (file, mut params) = split_command(cmdline);
        let lf = file.to_lowercase();
        if lf.ends_with("msiexec.exe") || lf == "msiexec" {
            // Registry often says /I{GUID} (repair) - we want removal.
            params = params
                .split_whitespace()
                .map(|t| {
                    let lt = t.to_lowercase();
                    if lt.starts_with("/i") { format!("/X{}", &t[2..]) } else { t.to_string() }
                })
                .collect::<Vec<_>>()
                .join(" ");
        }
        match shell_run_wait(&file, &params, None) {
            // 1605 = MSI "product not installed"; 3010 = success, reboot needed.
            Ok(code) => Outcome {
                ok: code == 0 || code == 3010 || code == 1605,
                code,
                message: match code {
                    0 => "Uninstaller finished.".into(),
                    3010 => "Uninstalled - a restart is needed to finish.".into(),
                    1602 => "Uninstall was cancelled.".into(),
                    c => format!("Uninstaller exited with code {c}."),
                },
            },
            Err(e) => Outcome { ok: false, code: -1, message: e.to_string() },
        }
    }

    fn regkey_exists(full: &str) -> bool {
        let Some((hive, sub)) = split_hive(full) else { return false };
        RegKey::predef(hive).open_subkey_with_flags(sub, KEY_READ).is_ok()
    }

    fn split_hive(full: &str) -> Option<(HKEY, &str)> {
        let (h, rest) = full.split_once('\\')?;
        let hive = match h.to_uppercase().as_str() {
            "HKCU" | "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
            "HKLM" | "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
            _ => return None,
        };
        Some((hive, rest))
    }

    fn registry_leftovers(app: &App, all: &[App]) -> Vec<Leftover> {
        let keys = name_keys(app);
        let pubk = publisher_key(&app.publisher);
        let (other_names, _) = others_keys(app, all);
        let mut out = Vec::new();
        for (hive, hname) in [(HKEY_CURRENT_USER, "HKCU"), (HKEY_LOCAL_MACHINE, "HKLM")] {
            for base in ["SOFTWARE", r"SOFTWARE\WOW6432Node"] {
                let Ok(sw) = RegKey::predef(hive).open_subkey_with_flags(base, KEY_READ) else { continue };
                for sub in sw.enum_keys().flatten() {
                    let n = norm(&sub);
                    if keys.contains(&n) && !other_names.contains(&n) {
                        out.push(Leftover { kind: "regkey".into(), path: format!(r"{hname}\{base}\{sub}"), bytes: 0 });
                    } else if !pubk.is_empty() && n == pubk && !NEVER.contains(&n.as_str()) {
                        if let Ok(pk) = sw.open_subkey_with_flags(&sub, KEY_READ) {
                            for inner in pk.enum_keys().flatten() {
                                let inn = norm(&inner);
                                if keys.contains(&inn) && !other_names.contains(&inn) {
                                    out.push(Leftover { kind: "regkey".into(), path: format!(r"{hname}\{base}\{sub}\{inner}"), bytes: 0 });
                                }
                            }
                        }
                    }
                }
            }
        }
        // The uninstall entry itself, if the uninstaller forgot it.
        if !app.reg_key.is_empty() && regkey_exists(&app.reg_key) {
            out.push(Leftover { kind: "regkey".into(), path: app.reg_key.clone(), bytes: 0 });
        }
        out
    }

    pub fn leftovers(app: &App, all: &[App]) -> Vec<Leftover> {
        let e = |v: &str| util::env_path(v);
        let mut roots: Vec<PathBuf> = Vec::new();
        for v in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "APPDATA", "LOCALAPPDATA"] {
            if let Some(p) = e(v) {
                roots.push(p);
            }
        }
        if let Some(l) = e("LOCALAPPDATA") {
            roots.push(l.join("Programs"));
        }
        if let Some(u) = e("USERPROFILE") {
            roots.push(u.join(r"AppData\LocalLow"));
        }
        let mut menus = Vec::new();
        if let Some(a) = e("APPDATA") {
            menus.push(a.join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        if let Some(p) = e("ProgramData") {
            menus.push(p.join(r"Microsoft\Windows\Start Menu\Programs"));
        }
        roots.extend(menus.iter().cloned());
        let mut desk = menus.clone();
        if let Some(u) = e("USERPROFILE") {
            desk.push(u.join("Desktop"));
        }
        if let Some(p) = e("PUBLIC") {
            desk.push(p.join("Desktop"));
        }
        let lnk: &[&str] = &["lnk", "url"];
        let extra: Vec<(PathBuf, &[&str])> = desk.into_iter().map(|d| (d, lnk)).collect();
        let mut out = scan_dirs(app, all, &roots, &extra);
        if app.source != "store" {
            out.extend(registry_leftovers(app, all));
        }
        out
    }

    pub fn remove(items: &[Leftover]) -> RemoveReport {
        let mut rep = remove_files_to_trash(items);
        let regs: Vec<&Leftover> = items.iter().filter(|i| i.kind == "regkey").collect();
        if regs.is_empty() {
            return rep;
        }
        let dir = util::data_dir().join("backups");
        let _ = std::fs::create_dir_all(&dir);
        let stamp = util::now_secs();
        for (i, it) in regs.iter().enumerate() {
            let file = dir.join(format!("{stamp}-{i}.reg"));
            let file_s = file.to_string_lossy().to_string();
            let backed = util::run_capture("reg.exe", &["export", &it.path, &file_s, "/y"]).map(|(c, _)| c == 0).unwrap_or(false);
            if !backed {
                rep.failed.push((it.path.clone(), "could not back up key - left untouched".into()));
                continue;
            }
            rep.backup = Some(dir.to_string_lossy().into());
            let Some((hive, sub)) = split_hive(&it.path) else { continue };
            match RegKey::predef(hive).delete_subkey_all(sub) {
                Ok(()) => rep.removed.push(it.path.clone()),
                Err(e) => rep.failed.push((it.path.clone(), e.to_string())),
            }
        }
        rep
    }
}

// ================================================================ Linux ====

#[cfg(target_os = "linux")]
mod platform {
    use super::*;

    fn dpkg() -> Vec<App> {
        let Ok((_, out)) = util::run_capture(
            "dpkg-query",
            &["-W", "-f=${Package}\t${Version}\t${Installed-Size}\t${Maintainer}\t${db:Status-Abbrev}\t${binary:Summary}\n"],
        ) else {
            return vec![];
        };
        out.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                if f.len() < 5 {
                    return None;
                }
                let residual = f[4].trim() == "rc";
                Some(App {
                    id: format!("dpkg:{}", f[0]),
                    name: f[0].into(),
                    version: if residual { format!("{} (config only)", f[1]) } else { f[1].into() },
                    publisher: f[3].split('<').next().unwrap_or("").trim().into(),
                    size_bytes: f[2].trim().parse::<u64>().unwrap_or(0) * 1024,
                    source: "dpkg".into(),
                    can_uninstall: true,
                    icon: f.get(5).unwrap_or(&"").to_string(),
                    ..Default::default()
                })
            })
            .collect()
    }

    fn flatpak() -> Vec<App> {
        let Ok((0, out)) = util::run_capture("flatpak", &["list", "--app", "--columns=application,name,version,origin"]) else {
            return vec![];
        };
        out.lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() >= 2).then(|| App {
                    id: format!("flatpak:{}", f[0]),
                    name: f[1].into(),
                    version: f.get(2).unwrap_or(&"").to_string(),
                    publisher: f.get(3).unwrap_or(&"").to_string(),
                    source: "flatpak".into(),
                    can_uninstall: true,
                    ..Default::default()
                })
            })
            .collect()
    }

    fn snap() -> Vec<App> {
        let Ok((0, out)) = util::run_capture("snap", &["list"]) else { return vec![] };
        out.lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                (f.len() >= 5).then(|| App {
                    id: format!("snap:{}", f[0]),
                    name: f[0].into(),
                    version: f[1].into(),
                    publisher: f[4].into(),
                    source: "snap".into(),
                    can_uninstall: true,
                    ..Default::default()
                })
            })
            .collect()
    }

    pub fn list(_include_store: bool) -> Vec<App> {
        let mut v = dpkg();
        v.extend(flatpak());
        v.extend(snap());
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    pub fn uninstall(app: &App, _quiet: bool) -> Outcome {
        let (src, id) = app.id.split_once(':').unwrap_or(("", ""));
        let res = match src {
            "dpkg" if app.version.ends_with("(config only)") => util::run_capture("apt-get", &["purge", "-y", id]),
            "dpkg" => util::run_capture("apt-get", &["remove", "-y", id]),
            "flatpak" => util::run_capture("flatpak", &["uninstall", "-y", id]),
            "snap" => util::run_capture("snap", &["remove", id]),
            _ => return Outcome { ok: false, code: -1, message: "unknown package source".into() },
        };
        match res {
            Ok((code, out)) => Outcome { ok: code == 0, code, message: out.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n") },
            Err(e) => Outcome { ok: false, code: -1, message: e.to_string() },
        }
    }

    pub fn leftovers(app: &App, all: &[App]) -> Vec<Leftover> {
        let h = util::home();
        let roots = vec![h.join(".config"), h.join(".local/share"), h.join(".cache"), h.join(".var/app"), PathBuf::from("/opt")];
        let apps_dir = h.join(".local/share/applications");
        let desk: &[&str] = &["desktop"];
        scan_dirs(app, all, &roots, &[(apps_dir, desk)])
    }

    pub fn remove(items: &[Leftover]) -> RemoveReport {
        remove_files_to_trash(items)
    }
}

// ================================================================ macOS ====

#[cfg(target_os = "macos")]
mod platform {
    use super::*;

    pub fn list(_include_store: bool) -> Vec<App> {
        let mut v = Vec::new();
        for dir in [PathBuf::from("/Applications"), util::home().join("Applications")] {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "app").unwrap_or(false) {
                    let name = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
                    v.push(App {
                        id: format!("macos:{}", p.display()),
                        name,
                        size_bytes: fsutil::tree_size(&p).bytes,
                        install_location: p.to_string_lossy().into(),
                        source: "macos".into(),
                        can_uninstall: true,
                        ..Default::default()
                    });
                }
            }
        }
        v.sort_by_key(|a| a.name.to_lowercase());
        v
    }

    pub fn uninstall(app: &App, _quiet: bool) -> Outcome {
        match trash::delete(&app.install_location) {
            Ok(()) => Outcome { ok: true, code: 0, message: "Moved to Trash.".into() },
            Err(e) => Outcome { ok: false, code: -1, message: e.to_string() },
        }
    }

    pub fn leftovers(app: &App, all: &[App]) -> Vec<Leftover> {
        let l = util::home().join("Library");
        let roots: Vec<PathBuf> = ["Application Support", "Caches", "Logs", "Saved Application State", "Containers", "WebKit", "HTTPStorages"]
            .iter()
            .map(|d| l.join(d))
            .collect();
        let plist: &[&str] = &["plist"];
        scan_dirs(app, all, &roots, &[(l.join("Preferences"), plist)])
    }

    pub fn remove(items: &[Leftover]) -> RemoveReport {
        remove_files_to_trash(items)
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;
    pub fn list(_: bool) -> Vec<App> {
        vec![]
    }
    pub fn uninstall(_: &App, _: bool) -> Outcome {
        Outcome { ok: false, code: -1, message: "unsupported platform".into() }
    }
    pub fn leftovers(_: &App, _: &[App]) -> Vec<Leftover> {
        vec![]
    }
    pub fn remove(items: &[Leftover]) -> RemoveReport {
        remove_files_to_trash(items)
    }
}

pub fn list(include_store: bool) -> Vec<App> {
    platform::list(include_store)
}

pub fn uninstall(app: &App, quiet: bool) -> Outcome {
    platform::uninstall(app, quiet)
}

/// Leftovers for `app`, judged against every *other* program still installed.
pub fn leftovers(app: &App) -> Vec<Leftover> {
    let all = list(false);
    let mut v = platform::leftovers(app, &all);
    v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    v
}

pub fn remove_leftovers(items: &[Leftover]) -> RemoveReport {
    platform::remove(items)
}

#[cfg(windows)]
pub use platform::{shell_run_wait, split_command};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_stripping() {
        assert_eq!(norm(&strip_versions("Python 3.13 64-bit")), "python");
        assert_eq!(norm(&strip_versions("7-Zip 25.01 (x64)")), "7zip");
        assert_eq!(norm(&strip_versions("Thunderbird (x64 en-US)")), "thunderbird");
        assert_eq!(publisher_key("Dropbox, Inc."), "dropbox");
    }

    #[test]
    fn keys_skip_generic_names() {
        let a = App { name: "Microsoft".into(), ..Default::default() };
        assert!(name_keys(&a).is_empty());
        let b = App { name: "KeePassXC".into(), install_location: r"C:\Program Files\KeePassXC\".into(), ..Default::default() };
        assert_eq!(name_keys(&b)[0], "keepassxc");
        assert!(is_ancestor_or_same(Path::new("/home/me"), Path::new("/home/me/.config")));
        assert!(!is_ancestor_or_same(Path::new("/home/me/.config/x"), Path::new("/home/me")));
    }
}
