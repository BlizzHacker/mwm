//! Agentless file access inside Proxmox LXCs via the owning PVE node.
//! Small files travel as base64 over the authenticated MWM API.

use anyhow::{bail, Context};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};

use crate::server::{in_guest, in_guest_input};

const MAX_FILE: u64 = 8 * 1024 * 1024;

fn quoted(path: &str) -> anyhow::Result<String> {
    if !path.starts_with('/') || path.len() > 2048 || path.bytes().any(|b| b == 0 || b < 32 || b == 127) {
        bail!("use an absolute container path without control characters");
    }
    Ok(format!("'{}'", path.replace('\'', r"'\''")))
}

fn run(node: &str, vmid: &str, script: &str) -> anyhow::Result<String> {
    let (code, output) = in_guest(node, vmid, script)?;
    if code != 0 { bail!("LXC file operation failed: {}", output.trim().lines().last().unwrap_or("unknown error")); }
    Ok(output)
}

pub fn list(node: &str, vmid: &str, dir: &str) -> anyhow::Result<Value> {
    let dir = quoted(dir)?;
    let output = run(node, vmid, &format!(
        "cd {dir} && find . -mindepth 1 -maxdepth 1 -printf '%f\\0%y\\0%s\\0%T@\\0'"))?;
    let mut parts = output.split('\0');
    let mut entries = Vec::new();
    while let Some(name) = parts.next() {
        if name.is_empty() { break; }
        let kind = parts.next().context("incomplete LXC directory entry")?;
        let size = parts.next().context("incomplete LXC directory entry")?.parse::<u64>().unwrap_or(0);
        let modified = parts.next().context("incomplete LXC directory entry")?.parse::<f64>().unwrap_or(0.0) as u64;
        entries.push(json!({"name": name, "kind": kind, "size": size, "modified": modified}));
        if entries.len() >= 5000 { break; }
    }
    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(json!({"entries": entries}))
}

pub fn read(node: &str, vmid: &str, path: &str) -> anyhow::Result<Value> {
    let path = quoted(path)?;
    let output = run(node, vmid, &format!(
        "test -f {path} && test $(wc -c < {path}) -le {MAX_FILE} && base64 < {path} | tr -d '\\n'"))?;
    let data = output.trim();
    if data.len() > (MAX_FILE as usize * 4 / 3 + 8) { bail!("file exceeds 8 MiB transfer limit"); }
    Ok(json!({"data": data, "size": STANDARD.decode(data)?.len()}))
}

pub fn write(node: &str, vmid: &str, path: &str, data: &str) -> anyhow::Result<Value> {
    let path = quoted(path)?;
    if data.len() > (MAX_FILE as usize * 4 / 3 + 8) { bail!("file exceeds 8 MiB transfer limit"); }
    let decoded = STANDARD.decode(data)?;
    if decoded.len() as u64 > MAX_FILE { bail!("file exceeds 8 MiB transfer limit"); }
    // Bytes go through stdin: a single argv entry is capped at 128 KiB.
    let script = format!("tmp={path}.mwm-upload-$$; trap 'rm -f \"$tmp\"' EXIT; cat > \"$tmp\" && mv -f \"$tmp\" {path}");
    let (code, output) = in_guest_input(node, vmid, &script, &decoded)?;
    if code != 0 {
        bail!("LXC upload failed: {}", output.trim().lines().last().unwrap_or("unknown error"));
    }
    Ok(json!({"written": decoded.len()}))
}

pub fn mkdir(node: &str, vmid: &str, path: &str) -> anyhow::Result<Value> {
    let path = quoted(path)?;
    run(node, vmid, &format!("mkdir -- {path}"))?;
    Ok(json!({"ok": true}))
}

pub fn delete(node: &str, vmid: &str, path: &str) -> anyhow::Result<Value> {
    let path = quoted(path)?;
    run(node, vmid, &format!("if test -d {path}; then rmdir -- {path}; else rm -- {path}; fi"))?;
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_quoted_paths() {
        assert_eq!(quoted("/tmp/a'b").unwrap(), "'/tmp/a'\\''b'");
        assert!(quoted("relative").is_err());
        assert!(quoted("/tmp/a\nb").is_err());
    }
}
