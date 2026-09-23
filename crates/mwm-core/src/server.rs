//! Server insight for Proxmox VE, Unraid and plain Linux (also shows Docker /
//! SMART on Windows when the tools exist): guests, storage, ZFS, SMART, Docker,
//! failed services, kernels - plus a few guarded one-click actions.

use serde_json::{json, Value};

use crate::util;

fn run(prog: &str, args: &[&str]) -> Option<String> {
    util::run_capture(prog, args).ok().filter(|(c, _)| *c == 0).map(|(_, o)| o)
}

fn exists(p: &str) -> bool {
    std::path::Path::new(p).exists()
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if exists("/usr/bin/pveversion") {
        "proxmox"
    } else if exists("/etc/unraid-version") {
        "unraid"
    } else if exists("/etc/version") && std::fs::read_to_string("/etc/version").map(|s| s.contains("TrueNAS")).unwrap_or(false) {
        "truenas"
    } else {
        "linux"
    }
}

fn ini(path: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') && l.ends_with(']') {
            out.push((l.trim_matches(['[', ']', '"']).to_string(), Vec::new()));
        } else if let Some((k, v)) = l.split_once('=') {
            let kv = (k.trim().to_string(), v.trim().trim_matches('"').to_string());
            match out.last_mut() {
                Some(s) => s.1.push(kv),
                None => out.push((String::new(), vec![kv])),
            }
        }
    }
    out
}

fn proxmox() -> Value {
    if !exists("/usr/bin/pveversion") {
        return Value::Null;
    }
    let version = run("pveversion", &[]).unwrap_or_default().trim().to_string();
    let guests: Value = run("pvesh", &["get", "/cluster/resources", "--type", "vm", "--output-format", "json"])
        .and_then(|o| serde_json::from_str(&o).ok())
        .unwrap_or(json!([]));
    let storage: Vec<Value> = run("pvesm", &["status"])
        .map(|o| {
            o.lines()
                .skip(1)
                .filter_map(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    // Skip header noise and warnings like "unable to activate storage ...".
                    (f.len() >= 6 && matches!(f[2], "active" | "inactive" | "disabled")).then(|| json!({"name": f[0], "type": f[1], "status": f[2], "total": f[3].parse::<u64>().unwrap_or(0) * 1024, "used": f[4].parse::<u64>().unwrap_or(0) * 1024, "avail": f[5].parse::<u64>().unwrap_or(0) * 1024}))
                })
                .collect()
        })
        .unwrap_or_default();
    let subscription = run("pvesubscription", &["get"]).and_then(|o| o.lines().find_map(|l| l.trim().strip_prefix("status:").map(|s| s.trim().to_string()))).unwrap_or_default();
    json!({"version": version, "guests": guests, "storage": storage, "subscription": subscription})
}

fn unraid() -> Value {
    if !exists("/etc/unraid-version") {
        return Value::Null;
    }
    let version = std::fs::read_to_string("/etc/unraid-version").unwrap_or_default().replace("version=", "").trim().trim_matches('"').to_string();
    let var = ini("/var/local/emhttp/var.ini");
    let get = |k: &str| var.iter().flat_map(|s| s.1.iter()).find(|(kk, _)| kk == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let disks: Vec<Value> = ini("/var/local/emhttp/disks.ini")
        .into_iter()
        .map(|(name, kv)| {
            let g = |k: &str| kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone()).unwrap_or_default();
            json!({"name": name, "device": g("device"), "status": g("status"), "temp": g("temp"), "size": g("size").parse::<u64>().unwrap_or(0) * 1024, "free": g("fsFree").parse::<u64>().unwrap_or(0) * 1024, "fs": g("fsType"), "errors": g("numErrors")})
        })
        .filter(|d| d["device"].as_str().map(|s| !s.is_empty()).unwrap_or(false))
        .collect();
    json!({"version": version, "array": get("mdState"), "parity": get("sbSyncErrs"), "disks": disks})
}

fn zfs() -> Value {
    let Some(list) = run("zpool", &["list", "-H", "-p", "-o", "name,size,alloc,free,frag,cap,health"]) else { return json!([]) };
    let pools: Vec<Value> = list
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 7 {
                return None;
            }
            let status = run("zpool", &["status", f[0]]).unwrap_or_default();
            let scan = status.lines().find_map(|l| l.trim().strip_prefix("scan:").map(|s| s.trim().to_string())).unwrap_or_default();
            let errors = status.lines().find_map(|l| l.trim().strip_prefix("errors:").map(|s| s.trim().to_string())).unwrap_or_default();
            Some(json!({"name": f[0], "size": f[1].parse::<u64>().unwrap_or(0), "alloc": f[2].parse::<u64>().unwrap_or(0), "free": f[3].parse::<u64>().unwrap_or(0), "frag": f[4], "cap": f[5], "health": f[6], "scan": scan, "errors": errors}))
        })
        .collect();
    json!(pools)
}

fn docker() -> Value {
    let Some(ps) = run("docker", &["ps", "-a", "--format", "{{json .}}"]) else { return Value::Null };
    let containers: Vec<Value> = ps
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|c| json!({"name": c["Names"], "image": c["Image"], "state": c["State"], "status": c["Status"], "id": c["ID"]}))
        .collect();
    let df: Vec<Value> = run("docker", &["system", "df", "--format", "{{json .}}"])
        .map(|o| o.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|d| json!({"type": d["Type"], "size": d["Size"], "reclaimable": d["Reclaimable"], "count": d["TotalCount"]})).collect())
        .unwrap_or_default();
    json!({"containers": containers, "df": df})
}

fn smart() -> Value {
    let Some(scan) = run("smartctl", &["--scan", "-j"]) else { return Value::Null };
    let devs: Vec<String> = serde_json::from_str::<Value>(&scan)
        .ok()
        .and_then(|v| v["devices"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|d| d["name"].as_str().map(String::from))
        .collect();
    let disks: Vec<Value> = devs
        .iter()
        .filter_map(|d| {
            // smartctl exits non-zero for warnings; parse regardless.
            let (_, out) = util::run_capture("smartctl", &["-j", "-H", "-A", "-i", d]).ok()?;
            let v: Value = serde_json::from_str(&out).ok()?;
            let attr = |id: u64| v["ata_smart_attributes"]["table"].as_array().and_then(|t| t.iter().find(|a| a["id"].as_u64() == Some(id))).and_then(|a| a["raw"]["value"].as_u64());
            Some(json!({
                "device": d,
                "model": v["model_name"].as_str().or(v["model_family"].as_str()).unwrap_or(""),
                "capacity": v["user_capacity"]["bytes"].as_u64().or(v["nvme_total_capacity"].as_u64()),
                "passed": v["smart_status"]["passed"].as_bool(),
                "temp": v["temperature"]["current"].as_u64(),
                "hours": v["power_on_time"]["hours"].as_u64(),
                "wear": v["nvme_smart_health_information_log"]["percentage_used"].as_u64(),
                "reallocated": attr(5),
                "pending": attr(197),
                "media_errors": v["nvme_smart_health_information_log"]["media_errors"].as_u64(),
                "rotation": v["rotation_rate"].as_u64(),
            }))
        })
        .collect();
    json!(disks)
}

pub fn info() -> Value {
    let loadavg = std::fs::read_to_string("/proc/loadavg").ok().map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" ")).unwrap_or_default();
    let failed: Vec<String> = run("systemctl", &["--failed", "--no-legend", "--plain"])
        .map(|o| o.lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect())
        .unwrap_or_default();
    let kernel = run("uname", &["-r"]).unwrap_or_default().trim().to_string();
    let kernels: Vec<String> = run("dpkg-query", &["-W", "-f=${Package} ${db:Status-Abbrev}\\n", "proxmox-kernel-*", "pve-kernel-*", "linux-image-[0-9]*"])
        .map(|o| o.lines().filter(|l| l.contains(" ii")).filter_map(|l| l.split_whitespace().next().map(String::from)).filter(|p| p.chars().any(|c| c.is_ascii_digit()) && !p.ends_with("-signed")).collect())
        .unwrap_or_default();
    json!({
        "platform": platform(),
        "kernel": kernel,
        "kernels": kernels,
        "load": loadavg,
        "failed": failed,
        "proxmox": proxmox(),
        "unraid": unraid(),
        "zfs": zfs(),
        "docker": docker(),
        "smart": smart(),
    })
}

fn safe_token(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:".contains(c)) {
        anyhow::bail!("invalid target `{s}`");
    }
    Ok(s)
}

/// Guarded server actions. `target` is validated to a plain identifier.
pub fn action(action: &str, target: &str) -> anyhow::Result<String> {
    let t = safe_token(target)?;
    let (prog, args): (&str, Vec<String>) = match action {
        "guest_start" | "guest_shutdown" | "guest_reboot" => {
            // node/type/vmid through the cluster API, so guests on any node work.
            let parts: Vec<&str> = t.split('/').collect();
            let [node, kind, id] = parts.as_slice() else { anyhow::bail!("target must be <node>/<qemu|lxc>/<vmid>") };
            if !matches!(*kind, "qemu" | "lxc") || !id.chars().all(|c| c.is_ascii_digit()) {
                anyhow::bail!("bad guest target {t}");
            }
            let verb = action.trim_start_matches("guest_");
            ("pvesh", vec!["create".into(), format!("/nodes/{node}/{kind}/{id}/status/{verb}")])
        }
        "docker_start" | "docker_stop" | "docker_restart" => ("docker", vec![action.trim_start_matches("docker_").into(), t.into()]),
        "zfs_scrub" => ("zpool", vec!["scrub".into(), t.into()]),
        "smart_test" => ("smartctl", vec!["-t".into(), "short".into(), t.into()]),
        "service_restart" => ("systemctl", vec!["restart".into(), t.into()]),
        other => anyhow::bail!("unknown action {other}"),
    };
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, out) = util::run_capture(prog, &a)?;
    if code != 0 {
        anyhow::bail!("{prog} failed: {}", out.trim());
    }
    Ok(if out.trim().is_empty() { format!("{action} {t}: done") } else { out.trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(safe_token("pve1/qemu/100").is_ok());
        assert!(safe_token("sda; rm -rf /").is_err());
        assert!(safe_token("$(reboot)").is_err());
        assert!(action("nope", "x").is_err());
    }

    #[test]
    fn parses_ini() {
        let p = std::env::temp_dir().join(format!("mwm-ini-{}", std::process::id()));
        std::fs::write(&p, "[\"disk1\"]\nname=\"disk1\"\ndevice=\"sdb\"\ntemp=\"34\"\n").unwrap();
        let v = ini(&p.to_string_lossy());
        assert_eq!(v[0].0, "disk1");
        assert_eq!(v[0].1[1], ("device".to_string(), "sdb".to_string()));
        std::fs::remove_file(p).unwrap();
    }
}
