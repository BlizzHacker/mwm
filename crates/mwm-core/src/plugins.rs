//! Built-in service plugins for Proxmox LXCs. API keys are read inside their
//! owning host, used for local API calls, and never returned to the UI.

use std::time::Duration;

use anyhow::{bail, Context};
use serde_json::{json, Value};

use crate::util;

#[derive(Clone, Copy)]
struct Plugin {
    name: &'static str,
    port: u16,
    api: &'static str,
    config: &'static str,
}

fn plugin(name: &str) -> Option<Plugin> {
    match name.to_ascii_lowercase().as_str() {
        "sonarr" => Some(Plugin { name: "Sonarr", port: 8989, api: "v3", config: "/var/lib/sonarr/config.xml" }),
        "radarr" => Some(Plugin { name: "Radarr", port: 7878, api: "v3", config: "/var/lib/radarr/config.xml" }),
        "lidarr" => Some(Plugin { name: "Lidarr", port: 8686, api: "v1", config: "/var/lib/lidarr/config.xml" }),
        "prowlarr" => Some(Plugin { name: "Prowlarr", port: 9696, api: "v1", config: "/var/lib/prowlarr/config.xml" }),
        "maintainerr" => Some(Plugin { name: "Maintainerr", port: 6246, api: "health", config: "" }),
        "cleanuparr" => Some(Plugin { name: "CleanUpArr", port: 11011, api: "http", config: "" }),
        "rommarr" => Some(Plugin { name: "RomMarr", port: 80, api: "http", config: "" }),
        _ => None,
    }
}

fn capture(cmd: &str, args: &[&str]) -> anyhow::Result<String> {
    let (code, out) = util::run_capture(cmd, args)?;
    if code != 0 { bail!("{} exited with {code}", cmd); }
    Ok(out)
}

fn local_node() -> anyhow::Result<String> {
    Ok(capture("hostname", &[])?.trim().to_string())
}

fn ipv4(vmid: &str) -> anyhow::Result<String> {
    let out = capture("pct", &["exec", vmid, "--", "hostname", "-I"])?;
    out.split_whitespace().find(|s| s.parse::<std::net::Ipv4Addr>().is_ok())
        .map(str::to_string).context("guest has no IPv4 address")
}

fn api_key(vmid: &str, config: &str) -> anyhow::Result<String> {
    let xml = capture("pct", &["exec", vmid, "--", "cat", config])?;
    let key = xml.split_once("<ApiKey>").and_then(|(_, tail)| tail.split_once("</ApiKey>"))
        .map(|(key, _)| key.trim()).context("API key missing in guest config")?;
    if key.is_empty() { bail!("API key empty in guest config"); }
    Ok(key.to_string())
}

fn get_json(agent: &ureq::Agent, url: &str, key: &str) -> anyhow::Result<Value> {
    let mut req = agent.get(url);
    if !key.is_empty() { req = req.set("X-Api-Key", key); }
    let response = req.call().with_context(|| format!("service API failed at {url}"))?;
    Ok(serde_json::from_reader(response.into_reader())?)
}

fn details(vmid: &str, ip: &str, p: Plugin) -> Value {
    let base = format!("http://{ip}:{}", p.port);
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2)).timeout(Duration::from_secs(5)).build();
    if p.api == "v3" || p.api == "v1" {
        let result = (|| -> anyhow::Result<Value> {
            let key = api_key(vmid, p.config)?;
            let root = format!("{base}/api/{}", p.api);
            let status = get_json(&agent, &format!("{root}/system/status"), &key)?;
            let health = get_json(&agent, &format!("{root}/health"), &key).unwrap_or(json!([]));
            let queue = get_json(&agent, &format!("{root}/queue/status"), &key).unwrap_or(Value::Null);
            Ok(json!({
                "reachable": true,
                "version": status["version"],
                "branch": status["branch"],
                "health": health.as_array().map(Vec::len).unwrap_or(0),
                "queue": queue["totalCount"].as_u64().or_else(|| queue["count"].as_u64()),
            }))
        })();
        return result.unwrap_or_else(|e| json!({"reachable": false, "error": e.to_string()}));
    }
    let path = if p.api == "health" { "/api/health/ready" } else { "/" };
    let reachable = agent.get(&format!("{base}{path}")).call().is_ok();
    json!({"reachable": reachable})
}

/// Discover supported services that live on this Proxmox node.
pub fn list() -> anyhow::Result<Value> {
    if cfg!(not(target_os = "linux")) { return Ok(json!([])); }
    let node = local_node()?;
    let raw = capture("pvesh", &["get", "/cluster/resources", "--type", "vm", "--output-format", "json"])?;
    let resources: Vec<Value> = serde_json::from_str(&raw)?;
    let mut out = Vec::new();
    for r in resources {
        if r["node"].as_str() != Some(&node) || r["type"] != "lxc" { continue; }
        let name = r["name"].as_str().unwrap_or("");
        let Some(p) = plugin(name) else { continue; };
        let vmid = r["vmid"].as_u64().unwrap_or(0).to_string();
        let running = r["status"] == "running";
        let ip = if running { ipv4(&vmid).unwrap_or_default() } else { String::new() };
        let info = if running && !ip.is_empty() { details(&vmid, &ip, p) } else { json!({"reachable": false}) };
        out.push(json!({
            "plugin": name.to_ascii_lowercase(), "label": p.name, "vmid": vmid,
            "node": node, "state": r["status"], "ip": ip,
            "url": if ip.is_empty() { String::new() } else { format!("http://{ip}:{}", p.port) },
            "info": info,
        }));
    }
    out.sort_by(|a, b| a["label"].as_str().cmp(&b["label"].as_str()));
    Ok(json!(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry() {
        assert_eq!(plugin("Sonarr").unwrap().port, 8989);
        assert_eq!(plugin("Prowlarr").unwrap().api, "v1");
        assert!(plugin("unknown").is_none());
    }
}
