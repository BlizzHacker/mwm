//! Windows File Explorer takeover.
//!
//! MWM Commander is a full file manager (copy/cut/paste, multi-rename, pack,
//! search, sizes, hidden files, context menus). Turning the takeover ON makes
//! double-clicking any folder or drive in Explorer - and any "open folder"
//! shell action - launch MWM Commander at that path instead. HKCU only:
//! per-user, no admin, fully reversible. Explorer itself keeps running
//! (Win+E, taskbar, desktop icons still work); only the open-folder handler
//! changes, which is the safest way to "take over" without breaking Windows.

use anyhow::bail;
use serde_json::{json, Value};

#[cfg(windows)]
use crate::util;

/// The shell verbs that fire when a user opens a folder or a drive.
#[cfg(windows)]
const KEYS: &[&str] = &[
    r"HKCU\Software\Classes\Directory\shell\open",
    r"HKCU\Software\Classes\Drive\shell\open",
];

#[cfg(windows)]
fn reg(args: &[&str]) -> (i32, String) {
    util::run_capture("reg", args).unwrap_or((-1, "reg.exe failed".into()))
}

#[cfg(windows)]
pub fn status() -> anyhow::Result<Value> {
    let exe = std::env::current_exe()?;
    let exe_s = exe.to_string_lossy().to_string();
    let mut on = true;
    let mut keys = Vec::new();
    for k in KEYS {
        let (code, out) = reg(&["query", k, "/ve"]);
        let active = code == 0 && out.contains(&exe_s);
        on &= active;
        keys.push(json!({"key": k, "active": active}));
    }
    Ok(json!({"available": true, "enabled": on, "exe": exe_s, "keys": keys}))
}

#[cfg(windows)]
pub fn set_takeover(enabled: bool) -> anyhow::Result<Value> {
    let exe = std::env::current_exe()?;
    let exe_s = exe.to_string_lossy().to_string();
    for k in KEYS {
        if enabled {
            let cmd = format!("\"{exe_s}\" \"%1\"");
            let (code, out) = reg(&["add", k, "/ve", "/t", "REG_SZ", "/d", &cmd, "/f"]);
            if code != 0 { bail!("reg add {k} failed: {out}"); }
        } else {
            // Removing our key lets Explorer fall back to the system default.
            reg(&["delete", k, "/f"]);
        }
    }
    // Nudge the shell to notice without a logoff.
    let _ = util::run_capture("ie4uinit.exe", &["-show"]);
    status()
}

#[cfg(not(windows))]
pub fn status() -> anyhow::Result<Value> {
    Ok(json!({"available": false, "reason": "Explorer takeover is a Windows feature"}))
}

#[cfg(not(windows))]
pub fn set_takeover(_enabled: bool) -> anyhow::Result<Value> {
    bail!("Explorer takeover is a Windows feature")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_shape() {
        let s = status().unwrap();
        assert!(s.get("available").is_some());
        #[cfg(windows)]
        assert_eq!(s["keys"].as_array().unwrap().len(), KEYS.len());
    }
}
