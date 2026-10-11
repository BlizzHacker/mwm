//! HestiaCP management through the hosting LXC on the Proxmox cluster.
//! Every call runs `v-*` scripts inside the Hestia container via `in_guest`,
//! so it works from any node that can reach the container's owner. User and
//! domain names are validated strictly: they travel inside a shell command.
//! Long operations (Let's Encrypt, rebuild) run as MWM jobs.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use serde_json::{json, Value};

use crate::{jobs, server::in_guest, util};

/// Where Hestia lives. Override per install with MWM_HESTIA_NODE / MWM_HESTIA_VMID.
fn host() -> (String, String) {
    let node = util::env_path("MWM_HESTIA_NODE")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "Slimmm".to_string());
    let vmid = util::env_path("MWM_HESTIA_VMID")
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "160".to_string());
    (node, vmid)
}

fn user_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || s.len() > 64 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("bad Hestia user name");
    }
    Ok(s)
}

fn domain_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty()
        || s.len() > 253
        || s.starts_with('.')
        || s.ends_with('.')
        || s.contains("..")
        || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
    {
        bail!("bad domain name");
    }
    Ok(s)
}

fn record_id_ok(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || s.len() > 12 || !s.chars().all(|c| c.is_ascii_digit()) {
        bail!("bad DNS record id");
    }
    Ok(s)
}

fn script_ok(s: &str) -> anyhow::Result<&str> {
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || s.is_empty() || s.len() > 48 {
        bail!("bad script name");
    }
    Ok(s)
}

/// Run a Hestia `v-*` script and parse its JSON output.
fn hestia_json(script: &str, args: &[&str]) -> anyhow::Result<Value> {
    let (node, vmid) = host();
    let mut cmd = format!("/usr/local/hestia/bin/{}", script_ok(script)?);
    for a in args {
        cmd.push(' ');
        cmd.push('\'');
        cmd.push_str(&a.replace('\'', r"'\''"));
        cmd.push('\'');
    }
    cmd.push_str(" 'json' 2>&1"); // Hestia takes the json format flag LAST
    let (code, out) = in_guest(&node, &vmid, &cmd)?;
    let text = out.trim();
    let start = text.find(['{', '[']).context("no JSON in Hestia output")?;
    let body = &text[start..];
    let value: Value = serde_json::from_str(body).with_context(|| format!("Hestia {script}: {}", text.lines().last().unwrap_or("")))
        .map_err(|e| if code != 0 { anyhow::anyhow!("{e}: {text}") } else { e })?;
    if code != 0 {
        bail!("Hestia {script} exited with {code}: {}", text.lines().last().unwrap_or(""));
    }
    Ok(value)
}

/// Run a Hestia `v-*` script whose output is plain text.
fn hestia_run(script: &str, args: &[&str]) -> anyhow::Result<String> {
    let (node, vmid) = host();
    let mut cmd = format!("/usr/local/hestia/bin/{} ", script_ok(script)?);
    for a in args {
        cmd.push('\'');
        cmd.push_str(&a.replace('\'', r"'\''"));
        cmd.push('\'');
        cmd.push(' ');
    }
    cmd.push_str("2>&1");
    let (code, out) = in_guest(&node, &vmid, &cmd)?;
    if code != 0 {
        bail!("Hestia {script} exited with {code}: {}", out.trim().lines().last().unwrap_or(""));
    }
    Ok(out.trim().to_string())
}

// ------------------------------------------------------------ caching ----

struct Cache {
    at: Instant,
    value: Value,
}

fn cache() -> &'static Mutex<HashMap<String, Cache>> {
    static C: OnceLock<Mutex<HashMap<String, Cache>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached(key: &str, ttl: Duration, build: impl FnOnce() -> anyhow::Result<Value>) -> anyhow::Result<Value> {
    if let Ok(c) = cache().lock() {
        if let Some(hit) = c.get(key) {
            if hit.at.elapsed() < ttl {
                return Ok(hit.value.clone());
            }
        }
    }
    let value = build()?;
    if let Ok(mut c) = cache().lock() {
        c.insert(key.to_string(), Cache { at: Instant::now(), value: value.clone() });
    }
    Ok(value)
}

/// Drop cached reads after any change so the next list is fresh.
fn invalidate() {
    if let Ok(mut c) = cache().lock() {
        c.clear();
    }
}

// -------------------------------------------------------------- reads ----

/// Hestia users: name, contact, package, per-system domain counts, suspended.
pub fn users() -> anyhow::Result<Value> {
    cached("users", Duration::from_secs(60), || {
        let raw = hestia_json("v-list-users", &[])?;
        let mut out: Vec<Value> = raw
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(user, v)| {
                        json!({
                            "user": user,
                            "name": v["NAME"],
                            "email": v["CONTACT"],
                            "package": v["PACKAGE"],
                            "shell": v["SHELL"],
                            "suspended": v["SUSPENDED"] == "yes",
                            "web_domains": v["U_WEB_DOMAINS"].as_str().and_then(|s| s.parse::<u64>().ok()),
                            "dns_domains": v["U_DNS_DOMAINS"].as_str().and_then(|s| s.parse::<u64>().ok()),
                            "mail_domains": v["U_MAIL_DOMAINS"].as_str().and_then(|s| s.parse::<u64>().ok()),
                            "disk_mb": v["U_DISK"].as_str().and_then(|s| s.parse::<u64>().ok()),
                            "bandwidth_mb": v["U_BANDWIDTH"].as_str().and_then(|s| s.parse::<u64>().ok()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| a["user"].as_str().cmp(&b["user"].as_str()));
        Ok(json!({"users": out}))
    })
}

/// Every web domain of every user, flattened for the Command Center.
pub fn domains() -> anyhow::Result<Value> {
    cached("domains", Duration::from_secs(60), || {
        let raw_users = hestia_json("v-list-users", &[])?;
        let names: Vec<String> = raw_users.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        let mut out: Vec<Value> = Vec::new();
        let mut errors: Vec<String> = Vec::new();
        for user in names {
            if user_ok(&user).is_err() {
                continue;
            }
            match hestia_json("v-list-web-domains", &[&user]) {
                Ok(raw) => {
                    if let Some(m) = raw.as_object() {
                        for (domain, v) in m {
                            if domain_ok(&domain).is_err() {
                                continue;
                            }
                            out.push(json!({
                                "user": user,
                                "domain": domain,
                                "ip": v["IP"],
                                "ssl": v["SSL"] == "yes",
                                "letsencrypt": v["LETSENCRYPT"] == "yes",
                                "suspended": v["SUSPENDED"] == "yes",
                                "disk_mb": v["U_DISK"].as_str().and_then(|s| s.parse::<u64>().ok()),
                                "bandwidth_mb": v["U_BANDWIDTH"].as_str().and_then(|s| s.parse::<u64>().ok()),
                                "backend": v["BACKEND"],
                                "proxy": v["PROXY"],
                                "aliases": v["ALIAS"].as_str().unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>(),
                                "created": v["DATE"],
                            }));
                        }
                    }
                }
                Err(e) => errors.push(format!("{user}: {e}")),
            }
        }
        out.sort_by(|a, b| a["domain"].as_str().cmp(&b["domain"].as_str()));
        Ok(json!({"domains": out, "errors": errors}))
    })
}

/// DNS records for one domain.
pub fn dns(user: &str, domain: &str) -> anyhow::Result<Value> {
    user_ok(user)?;
    domain_ok(domain)?;
    let key = format!("dns:{user}:{domain}");
    cached(&key, Duration::from_secs(30), || {
        let raw = hestia_json("v-list-dns-records", &[user, domain])?;
        let mut out: Vec<Value> = raw
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(id, v)| {
                        json!({
                            "id": id,
                            "type": v["TYPE"],
                            "value": v["VALUE"],
                            "ttl": v["TTL"],
                            "suspended": v["SUSPENDED"] == "yes",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by_key(|r| r["id"].as_str().unwrap_or("").to_string());
        Ok(json!({"domain": domain, "records": out}))
    })
}

// ------------------------------------------------------------ actions ----

/// Immediate web-domain actions. Mutating calls must be listed in api::is_mutating.
pub fn action(user: &str, domain: &str, action: &str) -> anyhow::Result<Value> {
    user_ok(user)?;
    domain_ok(domain)?;
    match action {
        "suspend" | "unsuspend" => {
            hestia_run(&format!("v-{action}-web-domain"), &[user, domain])?;
            invalidate();
            Ok(json!({"ok": true, "action": action, "domain": domain}))
        }
        "rebuild" | "letsencrypt" => {
            let (script, title) = match action {
                "rebuild" => ("v-rebuild-web-domain", format!("Rebuild {domain}")),
                _ => ("v-add-letsencrypt-domain", format!("Let's Encrypt for {domain}")),
            };
            let user = user.to_string();
            let domain = domain.to_string();
            let job_domain = domain.clone();
            let id = jobs::start("hestia", &title, "command", move |job| {
                let (u, d) = (user.as_str(), domain.as_str());
                user_ok(u)?;
                domain_ok(d)?;
                let out = hestia_run(script, &[u, d])?;
                invalidate();
                job.line(out.clone());
                Ok(out)
            });
            Ok(json!({"ok": true, "action": action, "domain": job_domain, "job": id}))
        }
        other => bail!("unknown Hestia action `{other}` (suspend, unsuspend, rebuild, letsencrypt)"),
    }
}

/// Add a DNS record (A, CNAME, TXT, MX...).
pub fn dns_add(user: &str, domain: &str, rtype: &str, value: &str) -> anyhow::Result<Value> {
    user_ok(user)?;
    domain_ok(domain)?;
    if !rtype.chars().all(|c| c.is_ascii_alphanumeric()) || rtype.is_empty() || rtype.len() > 10 {
        bail!("bad record type");
    }
    if value.is_empty() || value.len() > 512 || value.contains(|c: char| c == '\'' || c == ';' || c == '&' || c == '|' || c == '`' || c == '$') {
        bail!("bad record value");
    }
    let out = hestia_run("v-add-dns-record", &[user, domain, &rtype.to_ascii_uppercase(), value])?;
    invalidate();
    Ok(json!({"ok": true, "output": out}))
}

/// Delete a DNS record by id.
pub fn dns_delete(user: &str, domain: &str, id: &str) -> anyhow::Result<Value> {
    user_ok(user)?;
    domain_ok(domain)?;
    record_id_ok(id)?;
    let out = hestia_run("v-delete-dns-record", &[user, domain, id])?;
    invalidate();
    Ok(json!({"ok": true, "output": out}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation() {
        assert!(user_ok("admin").is_ok());
        assert!(user_ok("gray-wolf_1").is_ok());
        assert!(user_ok("a b").is_err());
        assert!(user_ok("../etc").is_err());
        assert!(user_ok("admin;rm").is_err());
        assert!(domain_ok("moveweight.com").is_ok());
        assert!(domain_ok("sub.domain.co.uk").is_ok());
        assert!(domain_ok("-bad-.com").is_ok()); // Hestia itself rejects; shell-safe is what matters
        assert!(domain_ok("a.com; rm -rf /").is_err());
        assert!(domain_ok("a.com`id`").is_err());
        assert!(domain_ok("").is_err());
        assert!(record_id_ok("12").is_ok());
        assert!(record_id_ok("12a").is_err());
        assert!(script_ok("v-list-users").is_ok());
        assert!(script_ok("v-list-users; rm").is_err());
    }
}
