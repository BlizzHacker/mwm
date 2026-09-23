//! Manage other MWM installs (Proxmox / Unraid / Linux / other PCs running
//! `mwm serve`) from this one. Connections live in the local data dir; calls
//! are proxied by the engine (no browser CORS / mixed-content limits) and
//! authenticated with the server's access token in an `X-MWM-Token` header.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conn {
    pub id: String,
    pub name: String,
    pub url: String,
    pub token: String,
}

/// What the UI sees - never the token.
#[derive(Debug, Clone, Serialize)]
pub struct ConnView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub has_token: bool,
}

fn file() -> std::path::PathBuf {
    util::data_dir().join("connections.json")
}

fn load() -> Vec<Conn> {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn store(list: &[Conn]) -> anyhow::Result<()> {
    let path = file();
    std::fs::write(&path, serde_json::to_string_pretty(list)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn list() -> Vec<ConnView> {
    load().into_iter().map(|c| ConnView { has_token: !c.token.is_empty(), id: c.id, name: c.name, url: c.url }).collect()
}

/// Accepts `host`, `host:7777`, `http://host:7777/`, or the full sign-in link
/// `http://host:7777/?token=abc` (token pulled out of it).
pub fn normalize(url: &str, token: &str) -> (String, String) {
    let mut u = url.trim().to_string();
    let mut t = token.trim().to_string();
    if let Some((base, q)) = u.split_once('?') {
        if let Some(tok) = q.split('&').find_map(|kv| kv.strip_prefix("token=")) {
            if t.is_empty() {
                t = tok.to_string();
            }
        }
        u = base.to_string();
    }
    if !u.starts_with("http://") && !u.starts_with("https://") {
        u = format!("http://{u}");
    }
    let host_part = u.split("://").nth(1).unwrap_or("").split('/').next().unwrap_or("");
    if !host_part.contains(':') {
        u = u.replacen(host_part, &format!("{host_part}:7777"), 1);
    }
    (u.trim_end_matches('/').to_string(), t)
}

pub fn save(id: Option<String>, name: &str, url: &str, token: &str) -> anyhow::Result<ConnView> {
    let (url, token) = normalize(url, token);
    if name.trim().is_empty() {
        anyhow::bail!("give the machine a name");
    }
    let mut all = load();
    let id = id.filter(|s| !s.is_empty()).unwrap_or_else(|| format!("m{}", util::now_secs()));
    match all.iter_mut().find(|c| c.id == id) {
        Some(c) => {
            c.name = name.trim().into();
            c.url = url;
            if !token.is_empty() {
                c.token = token; // blank = keep the saved token
            }
        }
        None => {
            if token.is_empty() {
                anyhow::bail!("paste the access token (run `mwm serve --show-token` on the server)");
            }
            all.push(Conn { id: id.clone(), name: name.trim().into(), url, token });
        }
    }
    store(&all)?;
    Ok(list().into_iter().find(|c| c.id == id).expect("just saved"))
}

pub fn remove(id: &str) -> anyhow::Result<()> {
    let mut all = load();
    all.retain(|c| c.id != id);
    store(&all)
}

/// Run `cmd` on the remote machine `id`.
pub fn call(id: &str, cmd: &str, args: &Value) -> Result<Value, String> {
    if cmd.starts_with("remote_") || cmd.starts_with("conn_") {
        return Err("that command only runs locally".into());
    }
    let c = load().into_iter().find(|c| c.id == id).ok_or("unknown machine - it may have been removed")?;
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout(Duration::from_secs(600)).build();
    let resp = agent
        .post(&format!("{}/api/{cmd}", c.url))
        .set("X-MWM-Token", &c.token)
        .set("Content-Type", "application/json")
        .send_string(&args.to_string());
    match resp {
        Ok(r) => {
            let body = r.into_string().map_err(|e| e.to_string())?;
            if body.is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(&body).map_err(|e| format!("bad reply from {}: {e}", c.name))
            }
        }
        Err(ureq::Error::Status(401, _)) => Err(format!("{} rejected the access token - update it in Machines", c.name)),
        Err(ureq::Error::Status(_, r)) => Err(r.into_string().unwrap_or_else(|_| "remote error".into())),
        Err(e) => Err(format!("can't reach {} ({}): {}", c.name, c.url, e.kind())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_and_links() {
        assert_eq!(normalize("pve1", "t"), ("http://pve1:7777".into(), "t".into()));
        assert_eq!(normalize("http://10.0.0.5:7777/?token=abc", ""), ("http://10.0.0.5:7777".into(), "abc".into()));
        assert_eq!(normalize("https://mwm.example.com:443/", "x").0, "https://mwm.example.com:443");
        assert!(call("nope", "remote_call", &Value::Null).is_err());
    }
}
