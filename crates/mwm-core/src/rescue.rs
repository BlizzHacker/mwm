//! MWM Rescue catalog - the 2026 replacement for the Geek Squad MRI disc.
//!
//! Every MRI tool that cannot legally ship inside an MWM product is replaced:
//!   * builtin  - MWM already has this function (jump to the page)
//!   * bundled  - freely redistributable; ships inside the rescue ISOs
//!   * download - fetched from the vendor's official URL on demand (what MRI
//!                itself did through gslink.us), checksum-verified when pinned
//!
//! One catalog drives every surface: desktop app, server web UI, CLI, WinPE
//! ISO and the Linux rescue ISO all read this same list.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};

use crate::util;

#[derive(Clone, Copy)]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    /// The MRI tool (or category) this replaces.
    pub replaces: &'static str,
    pub category: &'static str,
    pub kind: &'static str, // builtin | bundled | download
    pub platform: &'static str, // windows | linux | mac | any
    /// builtin: UI page to open. download: official vendor URL.
    pub target: &'static str,
    pub license: &'static str,
    pub notes: &'static str,
    /// Optional pinned SHA-256 for downloads (empty = verify none, warn user).
    pub sha256: &'static str,
}

const fn tool(
    id: &'static str, name: &'static str, replaces: &'static str, category: &'static str,
    kind: &'static str, platform: &'static str, target: &'static str, license: &'static str,
    notes: &'static str,
) -> Tool {
    Tool { id, name, replaces, category, kind, platform, target, license, notes, sha256: "" }
}

pub const CATALOG: &[Tool] = &[
    // ------------------------------------------------- MRI core functions
    tool("fmod", "System file integrity (FMOD replacement)", "MRI FMOD.exe", "Diagnostics", "builtin", "any", "rescue-integrity", "MWM", "Baseline + drift scan of system files on Windows, Linux and macOS."),
    tool("face", "Health scan", "MRI FACE.exe / full scan", "Diagnostics", "builtin", "any", "health", "MWM", "One-click health scan: junk, updates, startup, security."),
    tool("procmon-gs", "Process analyzer", "MRI ProcessAnalyzer.exe", "Diagnostics", "builtin", "any", "perf", "MWM", "Running programs by memory, with end-task and malware-analyze."),
    tool("startup-gs", "Startup manager", "MRI StartupManager.exe", "Windows repair", "builtin", "any", "startup", "MWM", "Enable/disable startup items exactly like Task Manager."),
    tool("uninstall-gs", "Clean uninstaller", "MRI Uninstaller.exe", "Windows repair", "builtin", "any", "uninstaller", "MWM", "Uninstall plus leftover scan; registry keys backed up first."),
    tool("sysupdate-gs", "Update manager", "MRI SystemUpdater.exe", "Windows repair", "builtin", "any", "updates", "MWM", "OS package updates and MWM self-update."),
    tool("drivewipe", "Secure drive wipe", "Disk Management/MRI drive_wipe.exe", "Disk & data", "builtin", "any", "tools", "MWM", "Shred paths / wipe free space already built into MWM."),
    tool("lab", "Offline malware triage", "MRI + Webroot System Analyzer", "Malware", "builtin", "any", "lab", "MWM", "Static triage, entropy, IOCs, quarantine."),

    // ------------------------------------------------- bundled (open, redistributable)
    tool("sevenzip", "7-Zip (latest)", "Compression Utilities/7-Zip (2019)", "Tools", "bundled", "any", "https://www.7-zip.org/download.html", "LGPL", "Archive manager; bundled current in the rescue ISOs."),
    tool("npp", "Notepad++ (latest)", "Windows Tools/Misc/Notepad++ (2019)", "Tools", "bundled", "windows", "https://notepad-plus-plus.org/downloads/", "GPL-2.0", "Text/config editor."),
    tool("putty", "PuTTY (latest)", "Windows Tools/Misc/putty.exe (2019)", "Tools", "bundled", "windows", "https://www.chiark.greenend.org.uk/~sgtatham/putty/latest.html", "MIT", "SSH/serial client."),
    tool("memtest", "Memtest86+", "EZBOOT/MEMTEST.IMG (2011)", "Diagnostics", "bundled", "any", "https://memtest.org/", "GPL-2.0", "Boots as its own entry on both rescue ISOs."),
    tool("testdisk", "TestDisk + PhotoRec", "Seagate Recovery Suite / data recovery", "Disk & data", "bundled", "any", "https://www.cgsecurity.org/wiki/TestDisk_Download", "GPL-2.0", "Partition recovery + file carving; replaces the Seagate suite."),
    tool("smartmontools", "smartmontools (smartctl)", "WD Diagnostics / PC-Doctor disk checks", "Diagnostics", "bundled", "any", "https://www.smartmontools.org/", "GPL-2.0", "SMART health and self-tests on any drive brand."),
    tool("ntpwedit", "NT Password Editor", "Windows Fixes/Password issues", "Windows repair", "bundled", "windows", "https://cdslow.org.ru/en/ntpwedit/", "GPL", "Offline Windows password reset (WinPE)."),
    tool("chntpw", "chntpw", "Windows Fixes/Password issues", "Windows repair", "bundled", "linux", "https://pogostick.net/~pnh/ntpasswd/", "GPL", "Offline Windows SAM reset from the Linux rescue ISO."),
    tool("cmospwd", "CmosPwd", "Windows Tools/Misc/CmosPwd", "Diagnostics", "bundled", "any", "https://www.cgsecurity.org/wiki/CmosPwd", "GPL-2.0", "BIOS/CMOS password recovery."),
    tool("nwipe", "nwipe (DBAN method)", "EZBOOT/DBAN", "Disk & data", "bundled", "linux", "https://github.com/martijnvanbrummelen/nwipe", "GPL-2.0", "NIST-grade disk erase; DBAN itself is abandoned."),
    tool("stressng", "stress-ng", "Diagnostic/Prime95", "Diagnostics", "bundled", "linux", "https://github.com/ColinIanKing/stress-ng", "GPL-2.0", "CPU/memory stress + burn-in."),
    tool("lshm", "Libre Hardware Monitor", "CPU-Z / HWMonitor", "Diagnostics", "bundled", "windows", "https://github.com/LibreHardwareMonitor/LibreHardwareMonitor", "MPL-2.0", "Open-source sensors replacing CPU-Z/HWMonitor."),
    tool("clamav", "ClamAV", "Malware core scanners (offline)", "Malware", "bundled", "any", "https://www.clamav.net/", "GPL-2.0", "Open-source engine; also powers MWM lab av_scan."),
    tool("hwinfo", "hwinfo/lshw", "PC-Doctor hardware report", "Diagnostics", "bundled", "linux", "https://github.com/lkundrak/lshw", "GPL", "Full hardware inventory on the Linux disc."),

    // ------------------------------------------- runtime downloads (vendor URLs)
    tool("stinger", "McAfee Stinger", "Malware/Core Scanners", "Malware", "download", "windows", "https://www.mcafee.com/en-us/consumer-corporate/mcafee-labs/free-tools/stinger.html", "Freeware", "On-demand targeted malware remover; fetched from McAfee when used."),
    tool("npe", "Norton Power Eraser", "Malware/Core Scanners", "Malware", "download", "windows", "https://support.norton.com/sp/en/us/home/current/solutions/v3672136", "Freeware", "Aggressive crimeware remover; official Norton download."),
    tool("adwcleaner", "Malwarebytes AdwCleaner", "Malware/Utilities (Kaspersky, MBAM)", "Malware", "download", "windows", "https://www.malwarebytes.com/adwcleaner", "Freeware", "PUP/adware cleaner replacing the dead Kaspersky/MBAM entries."),
    tool("kvrt", "Kaspersky Virus Removal Tool", "Kaspersky TDSSKiller (DO NOT USE)", "Malware", "download", "windows", "https://support.kaspersky.com/common/disinfection/5350", "Freeware", "Current KVRT replaces the 2011 TDSSKiller."),
    tool("bdremoval", "Bitdefender Uninstall/Removal Tool", "Bitdefender entries", "Malware", "download", "windows", "https://www.bitdefender.com/support/how-to-uninstall-bitdefender-security-software-1067.html", "Freeware", "Clean removal of broken Bitdefender installs."),
    tool("mcprr", "McAfee Consumer Product Removal (MCPR)", "Malware/Product Fixes/McAfee MCPR.exe", "Malware", "download", "windows", "https://www.mcafee.com/support/s/article?id=TS101331", "Freeware", "Official McAfee removal tool."),
    tool("nrt", "Norton Remove and Reinstall", "Symantec Norton_Removal_Tool.exe", "Malware", "download", "windows", "https://support.norton.com/sp/en/us/home/current/solutions/kb20080710133834EN", "Freeware", "Official Norton removal tool."),
    tool("sysinternals", "Sysinternals Suite", "procmon.zip + Debugging Tools", "Tools", "download", "windows", "https://learn.microsoft.com/en-us/sysinternals/downloads/sysinternals-suite", "MS EULA", "Procmon, Autoruns, TCPView etc., always current from Microsoft."),
    tool("clonezilla", "Clonezilla", "Acronis TrueImage", "Disk & data", "download", "linux", "https://clonezilla.org/downloads", "GPL-2.0", "Imaging/cloning replacing licensed Acronis; bootable on Linux disc."),
];

fn kind_ok(s: &str) -> bool {
    matches!(s, "builtin" | "bundled" | "download")
}

/// The catalog filtered for the platform MWM is running on.
pub fn catalog() -> Value {
    let here = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "mac" } else { "linux" };
    let items: Vec<Value> = CATALOG
        .iter()
        .filter(|t| t.platform == "any" || t.platform == here)
        .map(|t| json!({
            "id": t.id, "name": t.name, "replaces": t.replaces, "category": t.category,
            "kind": t.kind, "platform": t.platform, "target": t.target,
            "license": t.license, "notes": t.notes,
            "installed": download_dir(t.id).join("READY").exists(),
        }))
        .collect();
    json!({"platform": here, "tools": items})
}

fn id_ok(s: &str) -> anyhow::Result<&'static Tool> {
    if s.len() > 40 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("bad tool id");
    }
    CATALOG.iter().find(|t| t.id == s).context("unknown rescue tool")
}

pub fn download_dir(id: &str) -> PathBuf {
    util::data_dir().join("rescue").join(id)
}

/// Download a runtime tool from its official vendor page. Vendor pages are
/// HTML (not direct binaries for most AV tools), so we save the page +
/// extract direct .exe/.zip links when present, and mark the tool READY once
/// the user has the file. Freely-redistributable direct downloads are saved
/// straight into the rescue folder.
pub fn fetch(id: &str) -> anyhow::Result<Value> {
    let t = id_ok(id)?;
    if !kind_ok(t.kind) || t.kind != "download" {
        bail!("{} is {} - nothing to download", t.id, t.kind);
    }
    let dir = download_dir(t.id);
    std::fs::create_dir_all(&dir)?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout(Duration::from_secs(60))
        .build();
    let resp = agent.get(t.target).set("User-Agent", "MWM-Rescue/5.3").call()
        .with_context(|| format!("cannot reach {}", t.target))?;
    let status = resp.status();
    let ctype = resp.header("content-type").unwrap_or("").to_string();
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes)?;
    let mut saved = Vec::new();
    if ctype.contains("octet-stream") || ctype.contains("zip") || bytes.starts_with(&[0x4d, 0x5a]) || bytes.starts_with(b"PK\x03\x04") {
        // Direct binary download.
        let name = t.target.rsplit('/').next().filter(|s| s.len() < 200).unwrap_or("tool.bin");
        let path = dir.join(name);
        std::fs::write(&path, &bytes)?;
        let hash = hex(&ring_sha256(&bytes));
        if !t.sha256.is_empty() && !t.sha256.eq_ignore_ascii_case(&hash) {
            let _ = std::fs::remove_file(&path);
            bail!("checksum mismatch for {name} - file deleted (expected {}, got {hash})", t.sha256);
        }
        std::fs::write(dir.join("READY"), format!("{} {} {status} {hash}", name, bytes.len()))?;
        saved.push(json!({"file": name, "size": bytes.len(), "sha256": hash}));
    } else {
        // Vendor HTML page: keep it offline and list the direct-download links.
        let html = String::from_utf8_lossy(&bytes).to_string();
        std::fs::write(dir.join("vendor.html"), &html)?;
        let mut links: Vec<String> = Vec::new();
        let mut rest = html.as_str();
        while let Some(i) = rest.find("href=\"") {
            rest = &rest[i + 6..];
            let url = &rest[..rest.find('"').unwrap_or(rest.len())];
            let lower = url.to_lowercase();
            if lower.ends_with(".exe") || lower.ends_with(".zip") || lower.contains("download") {
                if url.starts_with("http") && links.len() < 25 && !links.iter().any(|l| l == url) {
                    links.push(url.to_string());
                }
            }
        }
        std::fs::write(dir.join("READY"), format!("vendor page + {} download link(s)", links.len()))?;
        saved.push(json!({"vendor_page": t.target, "direct_links": links}));
    }
    Ok(json!({"ok": true, "id": t.id, "dir": dir.to_string_lossy(), "saved": saved}))
}

/// Where a downloaded/bundled tool lives (UI opens it from here).
pub fn locate(id: &str) -> anyhow::Result<Value> {
    let t = id_ok(id)?;
    let dir = download_dir(t.id);
    let mut files = Vec::new();
    if dir.is_dir() {
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().to_string();
            if name == "READY" { continue; }
            files.push(json!({"name": name, "size": e.metadata().map(|m| m.len()).unwrap_or(0), "path": e.path().to_string_lossy()}));
        }
    }
    Ok(json!({"id": t.id, "kind": t.kind, "target": t.target, "dir": dir.to_string_lossy(), "files": files}))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// SHA-256 of downloaded bytes (same crate the lab module uses).
fn ring_sha256(data: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    sha2::Sha256::digest(data).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_shape() {
        // The full catalog is >= 20 on every platform; the filtered view only
        // carries this platform's tools (Windows-only AV entries drop on Linux).
        assert!(CATALOG.len() >= 20, "{} tools in CATALOG", CATALOG.len());
        for t in CATALOG {
            assert!(kind_ok(t.kind), "{} kind {}", t.id, t.kind);
            assert!(!t.name.is_empty() && !t.target.is_empty(), "{}", t.id);
            assert!(matches!(t.platform, "any" | "windows" | "linux" | "mac"), "{}", t.id);
        }
        let c = catalog();
        let tools = c["tools"].as_array().unwrap();
        let here = c["platform"].as_str().unwrap();
        assert!(!tools.is_empty());
        for t in tools {
            let plat = t["platform"].as_str().unwrap();
            assert!(plat == "any" || plat == here, "{} leaked into {}", t["id"], here);
        }
        // Every MRI replacement id resolves.
        assert!(id_ok("fmod").is_ok());
        assert!(id_ok("stinger").is_ok());
        assert!(id_ok("../../etc").is_err());
        assert!(id_ok("nope").is_err());
    }
}
