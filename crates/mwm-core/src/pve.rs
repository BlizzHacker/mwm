//! Proxmox VE cluster management through the cluster API (`pvesh`), which
//! answers for every node from any node: guests, snapshots, backups,
//! migration, storage, tasks, HA and backup coverage. Long operations return
//! a Proxmox task (UPID) that is followed as an MWM job, log streamed.

use std::collections::HashMap;

use anyhow::{bail, Context};
use serde_json::{json, Value};

use crate::{jobs, util};

fn pvesh(args: &[&str]) -> anyhow::Result<Value> {
    let mut a = args.to_vec();
    a.extend(["--output-format", "json"]);
    let (code, out) = util::run_capture("pvesh", &a)?;
    if code != 0 {
        bail!("{}", out.trim().lines().last().unwrap_or("pvesh failed"));
    }
    // pvesh may print warnings before the JSON on stderr (merged by run_capture).
    let start = out.find(['[', '{', '"']).unwrap_or(0);
    let body = out[start..].trim();
    if body.is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(body).unwrap_or_else(|_| Value::String(body.to_string())))
}

fn get(path: &str) -> Value {
    pvesh(&["get", path]).unwrap_or(Value::Null)
}

fn node_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        bail!("bad node name {s}");
    }
    Ok(s)
}

fn kind_ok(s: &str) -> anyhow::Result<&str> {
    match s {
        "qemu" | "lxc" => Ok(s),
        _ => bail!("guest type must be qemu or lxc"),
    }
}

fn id_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || s.len() > 9 || !s.chars().all(|c| c.is_ascii_digit()) {
        bail!("bad guest id {s}");
    }
    Ok(s)
}

fn name_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || s.len() > 40 || !s.chars().next().unwrap().is_ascii_alphabetic() || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        bail!("names must start with a letter and use only letters, digits, - and _");
    }
    Ok(s)
}

pub fn available() -> bool {
    std::path::Path::new("/usr/bin/pvesh").exists()
}

/// Everything for the cluster dashboard in one call.
pub fn overview() -> anyhow::Result<Value> {
    if !available() {
        bail!("this machine is not a Proxmox VE node");
    }
    let status = get("/cluster/status");
    let cluster = status.as_array().and_then(|a| a.iter().find(|x| x["type"] == "cluster")).cloned().unwrap_or(Value::Null);
    let res = pvesh(&["get", "/cluster/resources"]).unwrap_or(json!([]));
    let all = res.as_array().cloned().unwrap_or_default();
    let by = |t: &str| all.iter().filter(|r| r["type"] == t).cloned().collect::<Vec<_>>();
    let node_status: HashMap<String, Value> = status
        .as_array()
        .map(|a| a.iter().filter(|x| x["type"] == "node").map(|x| (x["name"].as_str().unwrap_or("").to_string(), x.clone())).collect())
        .unwrap_or_default();
    let nodes: Vec<Value> = by("node")
        .into_iter()
        .map(|mut n| {
            let name = n["node"].as_str().unwrap_or("").to_string();
            if let Some(st) = node_status.get(&name) {
                n["ip"] = st["ip"].clone();
                n["local"] = st["local"].clone();
            }
            n
        })
        .collect();

    // Last backup per guest from each node's vzdump task history.
    let mut last_backup: HashMap<String, Value> = HashMap::new();
    for n in &nodes {
        let node = n["node"].as_str().unwrap_or("");
        if n["status"] != "online" {
            continue;
        }
        let tasks = pvesh(&["get", &format!("/nodes/{node}/tasks"), "--typefilter", "vzdump", "--limit", "500"]).unwrap_or(Value::Null);
        for t in tasks.as_array().cloned().unwrap_or_default() {
            let id = t["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let ok = t["status"] == "OK";
            let start = t["starttime"].as_u64().unwrap_or(0);
            let e = last_backup.entry(id).or_insert(json!({"ok": 0, "failed": 0}));
            if ok && start > e["ok"].as_u64().unwrap_or(0) {
                e["ok"] = json!(start);
            }
            if !ok && start > e["failed"].as_u64().unwrap_or(0) {
                e["failed"] = json!(start);
                e["error"] = t["status"].clone();
            }
        }
    }
    let guests: Vec<Value> = by("qemu")
        .into_iter()
        .chain(by("lxc"))
        .map(|mut g| {
            let id = g["vmid"].to_string();
            g["backup"] = last_backup.get(&id).cloned().unwrap_or(Value::Null);
            g
        })
        .collect();
    let tasks = get("/cluster/tasks");
    let ha = get("/cluster/ha/status/current");
    let jobs = get("/cluster/backup");
    Ok(json!({
        "cluster": cluster,
        "quorate": cluster["quorate"],
        "nodes": nodes,
        "guests": guests,
        "storage": by("storage"),
        "tasks": tasks,
        "ha": ha,
        "backup_jobs": jobs,
    }))
}

/// Detail for one guest: live status, config, snapshots, backups on disk.
pub fn guest(node: &str, kind: &str, vmid: &str) -> anyhow::Result<Value> {
    let (node, kind, vmid) = (node_ok(node)?, kind_ok(kind)?, id_ok(vmid)?);
    let base = format!("/nodes/{node}/{kind}/{vmid}");
    let status = pvesh(&["get", &format!("{base}/status/current")])?;
    let config = get(&format!("{base}/config"));
    let snapshots = get(&format!("{base}/snapshot"));
    // Backups: every storage that holds backups and is active on this node.
    let mut backups = Vec::new();
    for s in get(&format!("/nodes/{node}/storage")).as_array().cloned().unwrap_or_default() {
        let content = s["content"].as_str().unwrap_or("");
        if !content.contains("backup") || s["active"] != 1 {
            continue;
        }
        let name = s["storage"].as_str().unwrap_or("");
        if let Ok(list) = pvesh(&["get", &format!("/nodes/{node}/storage/{name}/content"), "--content", "backup", "--vmid", vmid]) {
            for b in list.as_array().cloned().unwrap_or_default() {
                backups.push(json!({"storage": name, "volid": b["volid"], "size": b["size"], "ctime": b["ctime"], "notes": b["notes"], "protected": b["protected"]}));
            }
        }
    }
    backups.sort_by(|a, b| b["ctime"].as_u64().cmp(&a["ctime"].as_u64()));
    let storages: Vec<Value> = get(&format!("/nodes/{node}/storage"))
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s["content"].as_str().unwrap_or("").contains("backup") && s["active"] == 1)
        .map(|s| s["storage"].clone())
        .collect();
    let nodes: Vec<Value> = get("/cluster/status").as_array().cloned().unwrap_or_default().into_iter().filter(|x| x["type"] == "node" && x["online"] == 1).map(|x| x["name"].clone()).collect();
    Ok(json!({"node": node, "type": kind, "vmid": vmid, "status": status, "config": config, "snapshots": snapshots, "backups": backups, "backup_storages": storages, "nodes": nodes}))
}

/// Follow a Proxmox task (UPID) as an MWM job, streaming its log.
fn follow(node: &str, upid: &str, title: &str) -> u64 {
    let (node, upid, title) = (node.to_string(), upid.to_string(), title.to_string());
    jobs::start("proxmox", &title, "server", move |job| {
        let path = format!("/nodes/{node}/tasks/{upid}");
        let mut shown = 0usize;
        loop {
            if job.cancelled() {
                let _ = pvesh(&["delete", &path]);
                bail!("stopped");
            }
            let log = pvesh(&["get", &format!("{path}/log"), "--start", &shown.to_string(), "--limit", "500"]).unwrap_or(Value::Null);
            for l in log.as_array().cloned().unwrap_or_default() {
                job.line(l["t"].as_str().unwrap_or("").to_string());
                shown += 1;
            }
            let st = pvesh(&["get", &format!("{path}/status")])?;
            if st["status"] == "stopped" {
                let exit = st["exitstatus"].as_str().unwrap_or("").to_string();
                return if exit == "OK" { Ok("finished".into()) } else { bail!("{exit}") };
            }
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    })
}

fn upid_of(v: &Value) -> Option<String> {
    v.as_str().filter(|s| s.starts_with("UPID:")).map(String::from)
}

/// Guest operations. Returns {job} for long-running Proxmox tasks.
pub fn guest_op(node: &str, kind: &str, vmid: &str, op: &str, p: &Value) -> anyhow::Result<Value> {
    let (node, kind, vmid) = (node_ok(node)?, kind_ok(kind)?, id_ok(vmid)?);
    let base = format!("/nodes/{node}/{kind}/{vmid}");
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let (args, title): (Vec<String>, String) = match op {
        "start" | "shutdown" | "reboot" | "stop" | "suspend" | "resume" => (vec!["create".into(), format!("{base}/status/{op}")], format!("{op} {kind} {vmid}")),
        "snapshot" => {
            let name = s("name");
            name_ok(&name)?;
            let mut a = vec!["create".into(), format!("{base}/snapshot"), "--snapname".into(), name.clone()];
            let d = s("description");
            if !d.is_empty() {
                a.extend(["--description".into(), d.chars().take(200).collect()]);
            }
            if kind == "qemu" && p.get("vmstate").and_then(|v| v.as_bool()).unwrap_or(false) {
                a.extend(["--vmstate".into(), "1".into()]);
            }
            (a, format!("snapshot {vmid} -> {name}"))
        }
        "rollback" => {
            let name = s("name");
            name_ok(&name)?;
            (vec!["create".into(), format!("{base}/snapshot/{name}/rollback")], format!("rollback {vmid} to {name}"))
        }
        "delsnapshot" => {
            let name = s("name");
            name_ok(&name)?;
            (vec!["delete".into(), format!("{base}/snapshot/{name}")], format!("delete snapshot {name} of {vmid}"))
        }
        "backup" => {
            let storage = s("storage");
            node_ok(&storage).context("pick a backup storage")?;
            let mode = match s("mode").as_str() {
                "stop" => "stop",
                "suspend" => "suspend",
                _ => "snapshot",
            };
            (
                vec!["create".into(), format!("/nodes/{node}/vzdump"), "--vmid".into(), vmid.into(), "--storage".into(), storage.clone(), "--mode".into(), mode.into(), "--compress".into(), "zstd".into()],
                format!("backup {vmid} to {storage}"),
            )
        }
        "migrate" => {
            let target = s("target");
            node_ok(&target)?;
            if target == node {
                bail!("already on {node}");
            }
            let mut a = vec!["create".into(), format!("{base}/migrate"), "--target".into(), target.clone()];
            let running = p.get("running").and_then(|v| v.as_bool()).unwrap_or(false);
            if running {
                // VMs move live; containers must restart on the other node.
                a.extend(if kind == "qemu" { ["--online".into(), "1".into()] } else { ["--restart".into(), "1".into()] });
            }
            (a, format!("migrate {vmid} {node} -> {target}"))
        }
        other => bail!("unknown operation {other}"),
    };
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = pvesh(&a)?;
    match upid_of(&out) {
        Some(upid) => Ok(json!({"job": follow(node, &upid, &title), "upid": upid})),
        None => Ok(json!({"done": true, "message": format!("{title}: done")})),
    }
}

/// Log of any cluster task (from the Tasks list).
pub fn task_log(node: &str, upid: &str) -> anyhow::Result<Value> {
    node_ok(node)?;
    if !upid.starts_with("UPID:") || upid.contains([' ', '/', '\'', '"']) {
        bail!("bad task id");
    }
    let log = pvesh(&["get", &format!("/nodes/{node}/tasks/{upid}/log"), "--limit", "1000"])?;
    Ok(json!(log.as_array().cloned().unwrap_or_default().iter().map(|l| l["t"].clone()).collect::<Vec<_>>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_inputs() {
        assert!(node_ok("Thiccc").is_ok());
        assert!(node_ok("pve1; reboot").is_err());
        assert!(id_ok("104").is_ok());
        assert!(id_ok("104 && x").is_err());
        assert!(name_ok("before-upgrade_1").is_ok());
        assert!(name_ok("1bad").is_err());
        assert!(name_ok("a b").is_err());
        assert!(kind_ok("qemu").is_ok() && kind_ok("docker").is_err());
    }
}
