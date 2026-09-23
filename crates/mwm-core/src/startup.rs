//! Startup Manager: things that launch on their own.
//!
//! Windows run-key and startup-folder entries are toggled the same way Task
//! Manager does it (the `StartupApproved` flags), so nothing is ever deleted
//! and every change is reversible from Task Manager too.

use serde::{Deserialize, Serialize};

#[allow(unused_imports)]
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartupItem {
    pub id: String,
    /// run | folder | task | service | systemd | autostart
    pub kind: String,
    pub name: String,
    pub command: String,
    pub location: String,
    pub enabled: bool,
    /// user | machine
    pub scope: String,
    pub admin: bool,
    /// Service start mode / task state etc.
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub ok: bool,
    pub message: String,
}

impl Outcome {
    fn from(r: anyhow::Result<()>) -> Self {
        match r {
            Ok(()) => Outcome { ok: true, message: "Done".into() },
            Err(e) => Outcome { ok: false, message: e.to_string() },
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use anyhow::{bail, Context};
    use winreg::enums::*;
    use winreg::{RegKey, RegValue, HKEY};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    fn hive_of(scope: &str) -> (HKEY, &'static str) {
        if scope == "machine" {
            (HKEY_LOCAL_MACHINE, "HKLM")
        } else {
            (HKEY_CURRENT_USER, "HKCU")
        }
    }

    fn approved(hive: HKEY, sub: &str, name: &str) -> bool {
        RegKey::predef(hive)
            .open_subkey_with_flags(format!(r"{APPROVED}\{sub}"), KEY_READ)
            .and_then(|k| k.get_raw_value(name))
            .map(|v| v.bytes.first().map(|b| b % 2 == 0).unwrap_or(true))
            .unwrap_or(true)
    }

    fn set_approved(hive: HKEY, sub: &str, name: &str, enabled: bool) -> anyhow::Result<()> {
        let (k, _) = RegKey::predef(hive)
            .create_subkey(format!(r"{APPROVED}\{sub}"))
            .context("opening StartupApproved (administrator rights may be needed)")?;
        let mut bytes = vec![if enabled { 2u8 } else { 3u8 }, 0, 0, 0];
        if enabled {
            bytes.extend_from_slice(&[0u8; 8]);
        } else {
            // FILETIME of "now" - what Task Manager shows as "disabled on".
            let ft = (util::now_secs() + 11_644_473_600) * 10_000_000;
            bytes.extend_from_slice(&ft.to_le_bytes());
        }
        k.set_raw_value(name, &RegValue { bytes, vtype: REG_BINARY })?;
        Ok(())
    }

    fn run_items(out: &mut Vec<StartupItem>) {
        for (scope, path, approved_sub) in [("user", RUN, "Run"), ("machine", RUN, "Run"), ("machine", RUN32, "Run32")] {
            let (hive, hname) = hive_of(scope);
            let Ok(k) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
            for (name, _) in k.enum_values().flatten() {
                let command: String = k.get_value(&name).unwrap_or_default();
                out.push(StartupItem {
                    id: format!("run|{scope}|{approved_sub}|{name}"),
                    kind: "run".into(),
                    enabled: approved(hive, approved_sub, &name),
                    name,
                    command,
                    location: format!(r"{hname}\{path}"),
                    scope: scope.into(),
                    admin: scope == "machine",
                    detail: String::new(),
                });
            }
        }
    }

    fn folder_items(out: &mut Vec<StartupItem>) {
        let dirs = [
            ("user", util::env_path("APPDATA").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\Startup"))),
            ("machine", util::env_path("ProgramData").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\StartUp"))),
        ];
        for (scope, dir) in dirs {
            let Some(dir) = dir else { continue };
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let (hive, _) = hive_of(scope);
            for e in rd.flatten() {
                let file = e.file_name().to_string_lossy().to_string();
                if file.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                let stem = e.path().file_stem().unwrap_or_default().to_string_lossy().to_string();
                out.push(StartupItem {
                    id: format!("folder|{scope}|StartupFolder|{file}"),
                    kind: "folder".into(),
                    enabled: approved(hive, "StartupFolder", &file),
                    name: stem,
                    command: e.path().to_string_lossy().into(),
                    location: dir.to_string_lossy().into(),
                    scope: scope.into(),
                    admin: scope == "machine",
                    detail: String::new(),
                });
            }
        }
    }

    fn task_items(out: &mut Vec<StartupItem>) {
        let script = r#"Get-ScheduledTask | Where-Object { $_.TaskPath -notlike '\Microsoft\*' } | Select-Object TaskName,TaskPath,@{n='State';e={"$($_.State)"}},Author,@{n='Cmd';e={($_.Actions | ForEach-Object { "$($_.Execute) $($_.Arguments)".Trim() }) -join ' ; '}} | ConvertTo-Json -Compress"#;
        let Ok(rows) = util::powershell_json(script) else { return };
        for r in rows {
            let name = util::js_str(&r, "TaskName");
            let path = util::js_str(&r, "TaskPath");
            let state = util::js_str(&r, "State");
            out.push(StartupItem {
                id: format!("task|{path}{name}"),
                kind: "task".into(),
                name,
                command: util::js_str(&r, "Cmd"),
                location: path,
                enabled: state != "Disabled",
                scope: "machine".into(),
                admin: false,
                detail: format!("{state}{}", {
                    let a = util::js_str(&r, "Author");
                    if a.is_empty() { String::new() } else { format!(" - {a}") }
                }),
            });
        }
    }

    fn service_items(out: &mut Vec<StartupItem>) {
        let script = r#"Get-CimInstance Win32_Service | Where-Object { $_.PathName -and $_.PathName -notmatch '^"?[A-Za-z]:\\Windows\\' -and $_.PathName -notmatch 'Windows Defender|Windows Media Player' } | Select-Object Name,DisplayName,State,StartMode,PathName | ConvertTo-Json -Compress"#;
        let Ok(rows) = util::powershell_json(script) else { return };
        for r in rows {
            let mode = util::js_str(&r, "StartMode");
            out.push(StartupItem {
                id: format!("service|{}", util::js_str(&r, "Name")),
                kind: "service".into(),
                name: util::js_str(&r, "DisplayName"),
                command: util::js_str(&r, "PathName"),
                location: util::js_str(&r, "Name"),
                enabled: mode != "Disabled",
                scope: "machine".into(),
                admin: true,
                detail: format!("{mode} - {}", util::js_str(&r, "State")),
            });
        }
    }

    pub fn list() -> Vec<StartupItem> {
        let mut out = Vec::new();
        run_items(&mut out);
        folder_items(&mut out);
        std::thread::scope(|s| {
            let t = s.spawn(|| {
                let mut v = Vec::new();
                task_items(&mut v);
                v
            });
            let mut svc = Vec::new();
            service_items(&mut svc);
            out.extend(t.join().unwrap_or_default());
            out.extend(svc);
        });
        out
    }

    pub fn set(id: &str, enabled: bool) -> anyhow::Result<()> {
        let parts: Vec<&str> = id.splitn(4, '|').collect();
        match parts.as_slice() {
            ["run", scope, sub, name] | ["folder", scope, sub, name] => {
                let (hive, _) = hive_of(scope);
                set_approved(hive, sub, name, enabled)
            }
            ["task", full] => {
                let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
                let (code, out) = util::run_capture("schtasks.exe", &["/Change", "/TN", full, flag])?;
                if code != 0 {
                    bail!("{}", out.trim());
                }
                Ok(())
            }
            ["service", name] => set_service_mode(name, if enabled { "demand" } else { "disabled" }),
            _ => bail!("unknown startup item"),
        }
    }

    pub fn set_service_mode(name: &str, mode: &str) -> anyhow::Result<()> {
        let mode = match mode.to_lowercase().as_str() {
            "auto" | "automatic" => "auto",
            "manual" | "demand" => "demand",
            "disabled" => "disabled",
            "delayed-auto" => "delayed-auto",
            other => bail!("unknown start mode {other}"),
        };
        let (code, out) = util::run_capture("sc.exe", &["config", name, "start=", mode])?;
        if code != 0 {
            bail!("{} (administrator rights are needed for services)", out.trim());
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use anyhow::bail;

    pub fn list() -> Vec<StartupItem> {
        let mut out = Vec::new();
        if let Ok((_, text)) = util::run_capture(
            "systemctl",
            &["list-unit-files", "--type=service", "--state=enabled,disabled", "--no-legend", "--no-pager"],
        ) {
            for l in text.lines() {
                let f: Vec<&str> = l.split_whitespace().collect();
                if f.len() < 2 || f[0].contains('@') {
                    continue;
                }
                out.push(StartupItem {
                    id: format!("systemd|{}", f[0]),
                    kind: "systemd".into(),
                    name: f[0].trim_end_matches(".service").into(),
                    command: String::new(),
                    location: f[0].into(),
                    enabled: f[1] == "enabled",
                    scope: "machine".into(),
                    admin: true,
                    detail: f[1].into(),
                });
            }
        }
        let auto = util::home().join(".config/autostart");
        if let Ok(rd) = std::fs::read_dir(&auto) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x != "desktop").unwrap_or(true) {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).unwrap_or("").to_string();
                out.push(StartupItem {
                    id: format!("autostart|{}", p.display()),
                    kind: "autostart".into(),
                    name: { let n = field("Name"); if n.is_empty() { p.file_stem().unwrap_or_default().to_string_lossy().into() } else { n } },
                    command: field("Exec"),
                    location: p.to_string_lossy().into(),
                    enabled: field("Hidden") != "true" && field("X-GNOME-Autostart-enabled") != "false",
                    scope: "user".into(),
                    admin: false,
                    detail: String::new(),
                });
            }
        }
        out
    }

    pub fn set(id: &str, enabled: bool) -> anyhow::Result<()> {
        let (kind, rest) = id.split_once('|').unwrap_or(("", ""));
        match kind {
            "systemd" => {
                let (code, out) = util::run_capture("systemctl", &[if enabled { "enable" } else { "disable" }, rest])?;
                if code != 0 {
                    bail!("{}", out.trim());
                }
                Ok(())
            }
            "autostart" => {
                let text = std::fs::read_to_string(rest)?;
                let mut lines: Vec<String> = text.lines().filter(|l| !l.starts_with("Hidden=")).map(String::from).collect();
                if !enabled {
                    let at = lines.iter().position(|l| l.trim() == "[Desktop Entry]").map(|i| i + 1).unwrap_or(lines.len());
                    lines.insert(at, "Hidden=true".into());
                }
                std::fs::write(rest, lines.join("\n") + "\n")?;
                Ok(())
            }
            _ => bail!("unknown startup item"),
        }
    }

    pub fn set_service_mode(name: &str, mode: &str) -> anyhow::Result<()> {
        set(&format!("systemd|{name}"), mode != "disabled")
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub fn list() -> Vec<StartupItem> {
        let mut out = Vec::new();
        let dir = util::home().join("Library/LaunchAgents");
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                out.push(StartupItem {
                    id: format!("launchagent|{}", e.path().display()),
                    kind: "launchagent".into(),
                    name: e.path().file_stem().unwrap_or_default().to_string_lossy().into(),
                    location: dir.to_string_lossy().into(),
                    enabled: true,
                    scope: "user".into(),
                    ..Default::default()
                });
            }
        }
        out
    }
    pub fn set(_id: &str, _enabled: bool) -> anyhow::Result<()> {
        anyhow::bail!("not supported on this platform yet")
    }
    pub fn set_service_mode(_n: &str, _m: &str) -> anyhow::Result<()> {
        anyhow::bail!("not supported on this platform yet")
    }
}

pub fn list() -> Vec<StartupItem> {
    platform::list()
}

pub fn set_enabled(id: &str, enabled: bool) -> Outcome {
    Outcome::from(platform::set(id, enabled))
}

pub fn set_service_mode(name: &str, mode: &str) -> Outcome {
    Outcome::from(platform::set_service_mode(name, mode))
}
