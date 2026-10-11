//! FMOD replacement - system file integrity for every OS.
//!
//! Geek Squad's FMOD verified system files against a proprietary encrypted
//! database. MWM does the same with an open baseline: snapshot hashes of the
//! important system directories, then later scans report every file that
//! changed, vanished or appeared. Baseline lives in the MWM data dir as JSON;
//! scans run as jobs so the UI can stream progress.
//!
//! On Windows the baseline complements `sfc /scannow` (already a toolkit
//! task): SFC repairs against Microsoft's store, integrity detects drift
//! against *your* machine as you left it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Context;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{jobs, util};

/// Hard cap so a baseline never runs away on a huge volume.
const MAX_FILES: usize = 60_000;
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024; // hash only files up to 8 MiB; bigger = size/mtime only

fn baseline_path() -> PathBuf {
    util::data_dir().join("integrity-baseline.json")
}

/// Default watched roots per platform: the places malware and breakage live.
fn default_roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        vec![
            Path::new(&sys).join("System32"),
            Path::new(&sys).join("SysWOW64"),
            Path::new(&sys).join("WinSxS").join("Manifests"),
        ]
    } else if cfg!(target_os = "macos") {
        vec!["/usr/bin", "/usr/sbin", "/sbin", "/Library/LaunchDaemons", "/Library/LaunchAgents"]
            .into_iter().map(PathBuf::from).collect()
    } else {
        vec!["/usr/bin", "/usr/sbin", "/bin", "/sbin", "/lib", "/usr/lib/systemd/system", "/etc/systemd/system"]
            .into_iter().map(PathBuf::from).collect()
    }
}

struct Entry {
    size: u64,
    mtime: u64,
    hash: String, // empty when the file is larger than MAX_FILE_BYTES
}

fn mtime_of(p: &Path) -> u64 {
    fs::metadata(p).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs()).unwrap_or(0)
}

fn hash_file(p: &Path) -> String {
    use std::io::Read;
    let Ok(mut f) = fs::File::open(p) else { return String::new() };
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return String::new(),
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn walk(roots: &[PathBuf], job: Option<&jobs::Job>, sink: &mut BTreeMap<String, Entry>) {
    for root in roots {
        if !root.is_dir() { continue; }
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            if let Some(j) = job { if j.cancelled() { return; } }
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                let p = e.path();
                if ft.is_dir() {
                    // Skip pseudo/volatile trees.
                    let name = e.file_name().to_string_lossy().to_string();
                    if !(cfg!(windows) && name.eq_ignore_ascii_case("drivers") && dir.ends_with("System32")) {
                        stack.push(p);
                    }
                    continue;
                }
                if !ft.is_file() { continue; } // skip symlinks/sockets
                if sink.len() >= MAX_FILES { return; }
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                let hash = if size <= MAX_FILE_BYTES { hash_file(&p) } else { String::new() };
                sink.insert(p.to_string_lossy().to_string(), Entry { size, mtime: mtime_of(&p), hash });
            }
        }
    }
}

/// Snapshot the watched roots. Returns a job id; the job's message carries
/// the file count when done.
pub fn baseline(roots: Option<Vec<String>>) -> anyhow::Result<u64> {
    let roots: Vec<PathBuf> = roots.unwrap_or_default().into_iter().map(PathBuf::from).collect();
    let roots = if roots.is_empty() { default_roots() } else { roots };
    Ok(jobs::start("integrity", "Create integrity baseline", "rescue", move |job| {
        let mut map = BTreeMap::new();
        walk(&roots, Some(job), &mut map);
        let count = map.len();
        job.set_total(0, 0);
        let value = json!({
            "created": util::now_secs(),
            "roots": roots.iter().map(|r| r.to_string_lossy().to_string()).collect::<Vec<_>>(),
            "files": map.iter().map(|(k, v)| json!([k, v.size, v.mtime, v.hash])).collect::<Vec<_>>(),
        });
        let tmp = baseline_path().with_extension("tmp");
        fs::write(&tmp, value.to_string()).context("write baseline")?;
        fs::rename(&tmp, baseline_path()).context("commit baseline")?;
        job.line(format!("{count} files baselined across {} root(s)", roots.len()));
        Ok(format!("Baseline created: {count} files"))
    }))
}

/// Compare the live tree against the baseline as a job. Each changed file is
/// one output line (JSON), so the UI can render a table.
pub fn verify() -> anyhow::Result<u64> {
    let path = baseline_path();
    if !path.exists() {
        anyhow::bail!("no baseline yet - create one first");
    }
    Ok(jobs::start("integrity", "Verify system file integrity", "rescue", move |job| {
        let raw = fs::read_to_string(&path).context("read baseline")?;
        let base: Value = serde_json::from_str(&raw).context("parse baseline")?;
        let roots: Vec<PathBuf> = base["roots"].as_array().cloned().unwrap_or_default()
            .iter().filter_map(|v| v.as_str().map(PathBuf::from)).collect();
        let mut old: BTreeMap<String, (u64, u64, String)> = BTreeMap::new();
        for row in base["files"].as_array().cloned().unwrap_or_default() {
            if let (Some(p), Some(s), Some(m), Some(h)) = (row[0].as_str(), row[1].as_u64(), row[2].as_u64(), row[3].as_str()) {
                old.insert(p.to_string(), (s, m, h.to_string()));
            }
        }
        let mut now = BTreeMap::new();
        walk(&roots, Some(job), &mut now);

        let (mut changed, mut vanished, mut added) = (0u32, 0u32, 0u32);
        for (p, e) in &now {
            match old.get(p) {
                Some((os_, om, oh)) => {
                    let differs = e.size != *os_ || (!e.hash.is_empty() && !oh.is_empty() && e.hash != *oh) || e.mtime != *om;
                    if differs {
                        changed += 1;
                        if changed <= 2000 { job.line(json!({"state": "changed", "path": p}).to_string()); }
                    }
                }
                None => {
                    added += 1;
                    if added <= 2000 { job.line(json!({"state": "added", "path": p}).to_string()); }
                }
            }
        }
        for p in old.keys() {
            if !now.contains_key(p) {
                vanished += 1;
                if vanished <= 2000 { job.line(json!({"state": "vanished", "path": p}).to_string()); }
            }
        }
        let total = changed + vanished + added;
        job.line(json!({"state": "summary", "changed": changed, "vanished": vanished, "added": added, "scanned": now.len()}).to_string());
        Ok(if total == 0 {
            format!("All {} files match the baseline.", now.len())
        } else {
            format!("{total} difference(s): {changed} changed, {vanished} vanished, {added} added (of {} files)", now.len())
        })
    }))
}

/// Baseline metadata for the Rescue page (no file list).
pub fn status() -> Value {
    let p = baseline_path();
    let Ok(raw) = fs::read_to_string(&p) else {
        return json!({"available": false, "roots": default_roots().iter().map(|r| r.to_string_lossy().to_string()).collect::<Vec<_>>()});
    };
    let base: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    json!({
        "available": true,
        "created": base["created"],
        "roots": base["roots"],
        "files": base["files"].as_array().map(|a| a.len()).unwrap_or(0),
    })
}
