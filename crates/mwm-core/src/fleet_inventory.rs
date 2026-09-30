//! Read-only fleet inventory supplied by the local discovery job.
//! Only explicit public fields reach the browser; credentials in a source file
//! cannot accidentally become part of the MWM API response.
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::{fs, path::Path};

const MAX_BYTES: u64 = 4 * 1024 * 1024;

fn fields(source: &Value, names: &[&str]) -> Value {
    let mut out = Map::new();
    for name in names {
        if let Some(value) = source.get(*name) {
            if value.is_string() || value.is_number() || value.is_boolean() || value.is_null() {
                out.insert((*name).to_string(), value.clone());
            }
        }
    }
    Value::Object(out)
}

fn strings(source: &Value, key: &str, limit: usize) -> Vec<Value> {
    source[key].as_array().into_iter().flatten().take(limit)
        .filter_map(|v| v.as_str().map(|s| json!(s.chars().take(256).collect::<String>())))
        .collect()
}

fn project(source: &Value) -> Value {
    let array = |key: &str, limit: usize, allowed: &[&str]| -> Vec<Value> {
        source[key].as_array().into_iter().flatten().take(limit)
            .map(|v| fields(v, allowed)).collect()
    };
    let mut containers = array("containers", 4096,
        &["node", "vmid", "kind", "name", "status", "ipv4", "role", "owner", "health", "source"]);
    for (output, input) in containers.iter_mut().zip(source["containers"].as_array().into_iter().flatten()) {
        let apps = input["apps"].as_array().into_iter().flatten().take(64).filter_map(|a| {
            if a.is_string() { Some(fields(&json!({"name": a}), &["name"])) }
            else if a.is_object() { Some(fields(a, &["name", "url", "status", "type"])) }
            else { None }
        }).collect::<Vec<_>>();
        output["apps"] = json!(apps);
        output["tags"] = json!(strings(input, "tags", 32));
    }
    let routes = source["routes"].as_array().into_iter().flatten().take(4096).map(|route| {
        let mut safe = fields(route, &["name", "hostname", "rule", "service", "source", "product", "owner", "protocol", "health", "custom_domain_target", "migration_state"]);
        safe["hostnames"] = json!(strings(route, "hostnames", 32));
        safe["entrypoints"] = json!(strings(route, "entrypoints", 16));
        safe["targets"] = json!(strings(route, "targets", 32));
        safe["target_containers"] = json!(route["target_containers"].as_array().into_iter().flatten().take(16)
            .map(|v| fields(v, &["node", "vmid", "name"])).collect::<Vec<_>>());
        safe
    }).collect::<Vec<_>>();
    let services = source["services"].as_array().into_iter().flatten().take(4096).map(|service| {
        let mut safe = fields(service, &["name", "protocol", "source", "health"]);
        safe["targets"] = json!(strings(service, "targets", 32));
        safe
    }).collect::<Vec<_>>();
    let hestia = &source["hestia"];
    let domains = hestia["domains"].as_array().into_iter().flatten().take(4096).map(|d| {
        let mut safe = fields(d, &["domain", "owner", "product", "hosting_role", "proxy_target", "source"]);
        safe["aliases"] = json!(strings(d, "aliases", 32));
        safe
    }).collect::<Vec<_>>();
    let dns_zones = hestia["dns_zones"].as_array().into_iter().flatten().take(4096).map(|d| {
        let mut safe = fields(d, &["domain", "owner", "product", "source"]);
        safe["aliases"] = json!(strings(d, "aliases", 32));
        safe
    }).collect::<Vec<_>>();
    json!({
        "available": true,
        "generated_at": source.get("generated_at").filter(|v| v.is_string() || v.is_number()).cloned().unwrap_or(Value::Null),
        "cluster": fields(&source["cluster"], &["name", "proxmox_host"]),
        "nodes": array("nodes", 64, &["name", "node", "status", "ip", "role", "health", "cpu_used_fraction", "memory_total_bytes", "memory_used_bytes", "disk_total_bytes", "disk_used_bytes"]),
        "containers": containers,
        "routes": routes,
        "services": services,
        "hestia": {"domains": domains, "dns_zones": dns_zones},
    })
}

fn read_json(path: &Path) -> Result<Value> {
    if fs::metadata(path).context("inventory metadata")?.len() > MAX_BYTES {
        bail!("fleet inventory exceeds 4 MiB");
    }
    serde_json::from_slice(&fs::read(path).context("read fleet inventory")?)
        .context("parse fleet inventory")
}

fn project_plan(source: &Value) -> Value {
    let products = source["products"].as_array().into_iter().flatten().take(256).map(|p| {
        let mut safe = fields(p, &["id", "brand", "replacement_brand", "current_primary_domain", "desired_primary_domain", "current_web_target", "current_app_target", "migration_state", "hestia_role"]);
        safe["lxc"] = fields(&p["lxc"], &["node", "vmid"]);
        safe
    }).collect::<Vec<_>>();
    json!({"products": products, "created_at": source.get("created_at").filter(|v| v.is_string()).cloned().unwrap_or(Value::Null)})
}

pub fn read() -> Result<Value> {
    let path = std::env::var("MWM_FLEET_INVENTORY")
        .unwrap_or_else(|_| "/etc/mwm/fleet-inventory.json".to_string());
    let path = Path::new(&path);
    if !path.exists() {
        return Ok(json!({"available": false, "reason": "Fleet discovery has not published an inventory yet"}));
    }
    let source = read_json(path)?;
    let mut result = project(&source);
    let plan_path = std::env::var("MWM_DOMAIN_PLAN")
        .unwrap_or_else(|_| "/etc/mwm/domain-plan.json".to_string());
    if Path::new(&plan_path).exists() {
        result["domain_plan"] = project_plan(&read_json(Path::new(&plan_path))?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_public_inventory_fields_reach_browser() {
        let source = json!({
            "generated_at": "2026-09-27T00:00:00Z", "token": "private",
            "containers": [{"node":"pve1", "vmid": 190, "name":"mwm", "password":"private", "apps": [{"name":"Hestia", "url":"https://example.com", "api_key":"private"}]}],
            "hestia": {"domains": [{"domain":"example.com", "owner":"admin", "secret":"private", "aliases":["www.example.com"]}]}
        });
        let result = project(&source);
        assert_eq!(result["containers"][0]["apps"][0]["name"], "Hestia");
        assert_eq!(result["containers"][0]["password"], Value::Null);
        assert_eq!(result["containers"][0]["apps"][0]["api_key"], Value::Null);
        assert_eq!(result["hestia"]["domains"][0]["secret"], Value::Null);
        assert_eq!(result["token"], Value::Null);
    }
}
