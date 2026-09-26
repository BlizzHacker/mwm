//! Cluster-wide LXC update report and per-container automation policy.
//! The Proxmox host runs one timer; LXCs do not need an MWM agent.

use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::path::Path;

use crate::{jobs, pve, util};

const REPORT: &str = "/var/lib/mwm/lxc-updates.json";
const POLICY: &str = "/etc/mwm/lxc-update-policy.json";
const SCRIPT: &str = "/usr/local/libexec/mwm-lxc-updates";

pub fn report() -> anyhow::Result<Value> {
    let body = std::fs::read_to_string(REPORT)
        .context("No LXC update inventory yet; run the cluster update scanner")?;
    serde_json::from_str(&body).context("LXC update inventory is invalid")
}

pub fn policy() -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    match std::fs::read_to_string(POLICY) {
        Ok(body) => serde_json::from_str(&body).context("LXC update policy is invalid"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({"auto_apply": []})),
        Err(e) => Err(e.into()),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 9 && id.bytes().all(|b| b.is_ascii_digit())
}

pub fn set_policy(ids: &[String]) -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    if ids.len() > 100 || ids.iter().any(|id| !valid_id(id)) {
        bail!("policy contains an invalid container ID");
    }
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    let value = json!({"auto_apply": ids});
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

fn start(action: &str, node: &str, vmid: &str) -> anyhow::Result<Value> {
    if !pve::available() || !Path::new(SCRIPT).exists() {
        bail!("LXC update scanner is not installed on this Proxmox node");
    }
    if action == "apply" && (node.is_empty()
        || !node.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
        || !valid_id(vmid))
    {
        bail!("invalid node or container ID");
    }
    let action = action.to_string();
    let node = node.to_string();
    let vmid = vmid.to_string();
    let title = if action == "apply" {
        format!("Update CT {vmid} on {node}")
    } else {
        "Scan cluster container updates".to_string()
    };
    let id = jobs::start("lxc_update", &title, "cluster", move |job| {
        let args: Vec<&str> = if action == "apply" {
            vec![SCRIPT, "apply", &node, &vmid]
        } else {
            vec![SCRIPT, "scan", "--refresh"]
        };
        let (code, out) = util::run_capture("python3", &args)?;
        for line in out.lines().take(200) {
            job.line(line.to_string());
        }
        if code != 0 {
            bail!("LXC update command failed: {}", out.trim().lines().last().unwrap_or("unknown error"));
        }
        Ok(if action == "apply" {
            format!("CT {vmid} updated; pre-update snapshot retained")
        } else {
            "Cluster update inventory refreshed".to_string()
        })
    });
    Ok(json!({"job": id}))
}

pub fn scan() -> anyhow::Result<Value> {
    start("scan", "", "")
}

pub fn apply(node: &str, vmid: &str) -> anyhow::Result<Value> {
    start("apply", node, vmid)
}

#[cfg(test)]
mod tests {
    use super::valid_id;

    #[test]
    fn policy_ids_reject_path_and_shell_input() {
        assert!(valid_id("190"));
        assert!(!valid_id(""));
        assert!(!valid_id("../190"));
        assert!(!valid_id("190;id"));
    }
}
