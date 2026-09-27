//! Cluster-wide PVE host package updates. A single cluster timer inventories
//! nodes, and every apply is checked for quorum, backups and critical removals.

use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::path::Path;

use crate::{jobs, pve, util};

const REPORT: &str = "/var/lib/mwm/pve-updates.json";
const POLICY: &str = "/etc/mwm/pve-update-policy.json";
const SCRIPT: &str = "/usr/local/libexec/mwm-pve-updates";

pub fn report() -> anyhow::Result<Value> {
    let body = std::fs::read_to_string(REPORT)
        .context("No PVE host update inventory yet; run the cluster host scanner")?;
    serde_json::from_str(&body).context("PVE host update inventory is invalid")
}

pub fn policy() -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    match std::fs::read_to_string(POLICY) {
        Ok(body) => serde_json::from_str(&body).context("PVE host update policy is invalid"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({"auto_apply": []})),
        Err(e) => Err(e.into()),
    }
}

fn valid_node(node: &str) -> bool {
    !node.is_empty() && node.len() <= 64
        && node.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

pub fn set_policy(nodes: &[String]) -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    if nodes.len() > 32 || nodes.iter().any(|n| !valid_node(n)) {
        bail!("policy contains an invalid PVE node name");
    }
    let mut nodes = nodes.to_vec();
    nodes.sort();
    nodes.dedup();
    let value = json!({"auto_apply": nodes});
    let path = Path::new(POLICY);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&value)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o640))?;
    }
    std::fs::rename(tmp, path)?;
    Ok(value)
}

fn start(action: &str, node: &str) -> anyhow::Result<Value> {
    if !pve::available() || !Path::new(SCRIPT).exists() {
        bail!("PVE host update scanner is not installed on this Proxmox node");
    }
    if action == "apply" && !valid_node(node) {
        bail!("invalid PVE node name");
    }
    let action = action.to_owned();
    let node = node.to_owned();
    let title = if action == "apply" {
        format!("Update Proxmox host {node}")
    } else {
        "Scan Proxmox host updates".to_owned()
    };
    let id = jobs::start("pve_update", &title, "cluster", move |job| {
        let args: Vec<&str> = if action == "apply" {
            vec![SCRIPT, "apply", &node]
        } else {
            vec![SCRIPT, "scan", "--refresh"]
        };
        let (code, out) = util::run_capture("python3", &args)?;
        for line in out.lines().take(200) {
            job.line(line.to_owned());
        }
        if code != 0 {
            bail!("PVE update command failed: {}", out.trim().lines().last().unwrap_or("unknown error"));
        }
        Ok(if action == "apply" {
            format!("{node} packages updated; reboot remains a separate manual operation")
        } else {
            "Proxmox host inventory refreshed".to_owned()
        })
    });
    Ok(json!({"job": id}))
}

pub fn scan() -> anyhow::Result<Value> {
    start("scan", "")
}

pub fn apply(node: &str) -> anyhow::Result<Value> {
    start("apply", node)
}

#[cfg(test)]
mod tests {
    use super::valid_node;
    #[test]
    fn node_names_reject_command_input() {
        assert!(valid_node("Slimmm"));
        assert!(!valid_node(""));
        assert!(!valid_node("../node"));
        assert!(!valid_node("node;id"));
    }
}
