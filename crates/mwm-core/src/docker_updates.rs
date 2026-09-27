//! Agentless Docker Compose updates for projects running inside Proxmox LXCs.
//! The controller snapshots the LXC and verifies backup coverage before apply.

use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};

use crate::{jobs, pve, util};

const REPORT: &str = "/var/lib/mwm/docker-updates.json";
const POLICY: &str = "/etc/mwm/docker-update-policy.json";
const SCRIPT: &str = "/usr/local/libexec/mwm-docker-updates";

pub fn report() -> anyhow::Result<Value> {
    let body = std::fs::read_to_string(REPORT)
        .context("No Docker project inventory yet; run the cluster Docker scanner")?;
    serde_json::from_str(&body).context("Docker project inventory is invalid")
}

pub fn policy() -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    match std::fs::read_to_string(POLICY) {
        Ok(body) => serde_json::from_str(&body).context("Docker update policy is invalid"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({"auto_apply": []})),
        Err(e) => Err(e.into()),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 9 && id.bytes().all(|b| b.is_ascii_digit())
}

fn valid_node(node: &str) -> bool {
    !node.is_empty() && node.len() <= 64
        && node.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

fn valid_project(project: &str) -> bool {
    !project.is_empty() && project.len() <= 100
        && project.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

pub fn set_policy(items: &[Value]) -> anyhow::Result<Value> {
    if !pve::available() {
        bail!("this machine is not a Proxmox VE node");
    }
    if items.len() > 100 {
        bail!("too many Docker project policies");
    }
    let mut targets = BTreeSet::new();
    for item in items {
        let vmid = item.get("vmid").and_then(Value::as_str).unwrap_or("");
        let project = item.get("project").and_then(Value::as_str).unwrap_or("");
        if !valid_id(vmid) || !valid_project(project) {
            bail!("invalid Docker project policy");
        }
        targets.insert((vmid.to_owned(), project.to_owned()));
    }
    let value = json!({"auto_apply": targets.into_iter().map(|(vmid, project)| json!({"vmid": vmid, "project": project})).collect::<Vec<_>>()});
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

fn start(action: &str, node: &str, vmid: &str, project: &str) -> anyhow::Result<Value> {
    if !pve::available() || !Path::new(SCRIPT).exists() {
        bail!("Docker update scanner is not installed on this Proxmox node");
    }
    if action == "apply" && (!valid_node(node) || !valid_id(vmid) || !valid_project(project)) {
        bail!("invalid Docker project target");
    }
    let action = action.to_owned();
    let node = node.to_owned();
    let vmid = vmid.to_owned();
    let project = project.to_owned();
    let title = if action == "apply" {
        format!("Update Docker {project} in CT {vmid}")
    } else {
        "Scan Docker Compose projects in LXCs".to_owned()
    };
    let id = jobs::start("docker_update", &title, "cluster", move |job| {
        let args: Vec<&str> = if action == "apply" {
            vec![SCRIPT, "apply", &node, &vmid, &project]
        } else {
            vec![SCRIPT, "scan"]
        };
        let (code, out) = util::run_capture("python3", &args)?;
        for line in out.lines().take(200) {
            job.line(line.to_owned());
        }
        if code != 0 {
            bail!("Docker update failed: {}", out.trim().lines().last().unwrap_or("unknown error"));
        }
        Ok(if action == "apply" {
            format!("Docker Compose project {project} processed in CT {vmid}")
        } else {
            "Docker project inventory refreshed".to_owned()
        })
    });
    Ok(json!({"job": id}))
}

pub fn scan() -> anyhow::Result<Value> {
    start("scan", "", "", "")
}

pub fn apply(node: &str, vmid: &str, project: &str) -> anyhow::Result<Value> {
    start("apply", node, vmid, project)
}

#[cfg(test)]
mod tests {
    use super::{valid_id, valid_node, valid_project};
    #[test]
    fn targets_reject_shell_input() {
        assert!(valid_node("Slimmm"));
        assert!(valid_id("104"));
        assert!(valid_project("romm"));
        assert!(!valid_node("node;id"));
        assert!(!valid_id("../104"));
        assert!(!valid_project("romm;id"));
    }
}
