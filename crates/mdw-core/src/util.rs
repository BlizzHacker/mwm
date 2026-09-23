use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// A `Command` that never flashes a console window on Windows.
pub fn cmd(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Run a PowerShell snippet that ends in `ConvertTo-Json` and always hand back
/// an array (PowerShell collapses one-element arrays into a bare object).
pub fn powershell_json(script: &str) -> anyhow::Result<Vec<Value>> {
    let full = format!(
        "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; {script}"
    );
    let out = cmd("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim().trim_start_matches('\u{feff}');
    if text.is_empty() {
        return Ok(vec![]);
    }
    Ok(match serde_json::from_str::<Value>(text)? {
        Value::Array(a) => a,
        Value::Null => vec![],
        other => vec![other],
    })
}

pub fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn home() -> PathBuf {
    env_path("HOME")
        .or_else(|| env_path("USERPROFILE"))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn js_str(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// Where MDW keeps registry backups and logs.
pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        env_path("LOCALAPPDATA").unwrap_or_else(home)
    } else if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else {
        env_path("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))
    };
    // Kept apart from the install folder so an uninstall never takes backups with it.
    let dir = if cfg!(windows) { base.join("MoveWeight").join("MDW") } else { base.join("MDW") };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Lowercase alphanumerics only - used to compare app names with folder names.
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

pub fn run_capture(program: &str, args: &[&str]) -> anyhow::Result<(i32, String)> {
    let out = cmd(program).args(args).output()?;
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    let err = String::from_utf8_lossy(&out.stderr);
    if !err.trim().is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    Ok((out.status.code().unwrap_or(-1), text))
}
