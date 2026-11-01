//! Optional bridge to ry-ops/proxmox-mcp-server over loopback Streamable HTTP.
//! The endpoint stays on the Proxmox host; MWM's authenticated remote API is
//! the boundary used by desktop clients. The upstream server owns Proxmox API
//! credentials and enforces its read-only / role policy.

use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::util;

fn file() -> PathBuf { util::data_dir().join("mcp-proxmox.json") }

fn configured_url() -> String {
    std::fs::read_to_string(file()).ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["url"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// The upstream server is intentionally reached only through loopback on this
/// MWM host. This keeps its bearer/API credentials off the LAN.
fn normalize_url(raw: &str) -> Result<String> {
    let raw = raw.trim().trim_end_matches('/');
    let raw = raw.strip_prefix("http://").unwrap_or(raw);
    if raw.contains("https://") || raw.contains('@') || raw.contains('?') || raw.contains('#') || raw.contains('\\') {
        bail!("use a loopback Proxmox MCP address without credentials or query parameters");
    }
    let (hostport, path) = raw.split_once('/').unwrap_or((raw, ""));
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        let (h, tail) = rest.split_once(']').ok_or_else(|| anyhow::anyhow!("invalid loopback address"))?;
        (format!("[{h}]"), tail.strip_prefix(':').unwrap_or("8811"))
    } else {
        let (h, p) = hostport.split_once(':').unwrap_or((hostport, "8811"));
        (h.to_string(), p)
    };
    if !matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]") || port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
        bail!("Proxmox MCP must listen on this MWM host's loopback address (127.0.0.1:8811)");
    }
    if !path.is_empty() && path != "mcp" {
        bail!("the MCP endpoint path must be /mcp");
    }
    Ok(format!("http://{host}:{port}/mcp"))
}

fn response(body: &str, id: u64) -> Result<Value> {
    let candidates: Vec<&str> = if body.trim_start().starts_with('{') {
        vec![body.trim()]
    } else {
        body.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim).collect()
    };
    for candidate in candidates.iter().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(candidate) {
            if v["id"].as_u64() == Some(id) {
                if let Some(error) = v.get("error") {
                    bail!("Proxmox MCP: {}", error["message"].as_str().unwrap_or("request failed"));
                }
                return Ok(v["result"].clone());
            }
        }
    }
    bail!("Proxmox MCP returned no JSON-RPC reply")
}

struct Client {
    agent: ureq::Agent,
    url: String,
    session: Option<String>,
    next: u64,
}

impl Client {
    fn post(&self, message: Value, timeout: u64) -> Result<(String, Option<String>)> {
        let mut req = self.agent.post(&self.url)
            .timeout(Duration::from_secs(timeout))
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream");
        if let Some(session) = &self.session { req = req.set("Mcp-Session-Id", session); }
        match req.send_string(&message.to_string()) {
            Ok(reply) => {
                let session = reply.header("mcp-session-id").map(str::to_string);
                Ok((reply.into_string().unwrap_or_default(), session))
            }
            Err(ureq::Error::Status(401, _)) => bail!("Proxmox MCP rejected the request; check its local authentication configuration"),
            Err(ureq::Error::Status(code, _)) => bail!("Proxmox MCP returned HTTP {code}"),
            Err(e) => bail!("cannot reach Proxmox MCP: {e}"),
        }
    }

    fn connect(url: &str) -> Result<Self> {
        let mut client = Self {
            agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(3)).redirects(0).build(),
            url: normalize_url(url)?, session: None, next: 2,
        };
        let init = json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
            "protocolVersion":"2025-03-26", "capabilities":{},
            "clientInfo":{"name":"mwm", "version":crate::VERSION}
        }});
        let (body, session) = client.post(init, 10)?;
        response(&body, 1)?;
        client.session = session;
        let _ = client.post(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}), 5);
        Ok(client)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next;
        self.next += 1;
        let (body, _) = self.post(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}), 60)?;
        response(&body, id)
    }
}

fn client() -> Result<Client> {
    let url = configured_url();
    if url.is_empty() { bail!("connect the Proxmox MCP server on this machine first"); }
    Client::connect(&url)
}

pub fn status() -> Value {
    let url = configured_url();
    if url.is_empty() { return json!({"configured":false,"reachable":false,"url":"","tool_count":0,"error":""}); }
    match Client::connect(&url).and_then(|mut c| c.request("tools/list", json!({}))) {
        Ok(result) => {
            let tools = result["tools"].as_array().cloned().unwrap_or_default();
            let read_only = tools.iter().filter(|t| t["annotations"]["readOnlyHint"] == true).count();
            json!({"configured":true,"reachable":true,"url":url,"tool_count":tools.len(),"read_only_count":read_only,"error":""})
        }
        Err(e) => json!({"configured":true,"reachable":false,"url":url,"tool_count":0,"error":e.to_string()}),
    }
}

pub fn configure(raw: &str) -> Result<Value> {
    let url = normalize_url(raw)?;
    let mut c = Client::connect(&url).context("start proxmox-mcp-server-http on this machine, bound to 127.0.0.1")?;
    let result = c.request("tools/list", json!({}))?;
    if result["tools"].as_array().is_none_or(|tools| tools.is_empty()) {
        bail!("Proxmox MCP connected but did not advertise any tools");
    }
    let path = file();
    std::fs::write(&path, serde_json::to_vec_pretty(&json!({"url":url}))?)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(status())
}

pub fn tools() -> Result<Value> {
    let result = client()?.request("tools/list", json!({}))?;
    Ok(result["tools"].clone())
}

pub fn call(name: &str, args: &Value, write: bool) -> Result<Value> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid Proxmox MCP tool name");
    }
    if !args.is_object() && !args.is_null() { bail!("tool arguments must be an object"); }
    let mut client = client()?;
    let listed = client.request("tools/list", json!({}))?;
    let tool = listed["tools"].as_array()
        .and_then(|tools| tools.iter().find(|t| t["name"] == name))
        .ok_or_else(|| anyhow::anyhow!("Proxmox MCP does not advertise tool {name}"))?;
    let read_only = tool["annotations"]["readOnlyHint"] == true;
    if write && read_only { bail!("{name} is read-only; call it through MWM's read path"); }
    if !write && !read_only { bail!("{name} can change Proxmox; call it through the confirmed write path"); }
    let result = client.request("tools/call", json!({"name":name, "arguments":if args.is_null() { json!({}) } else { args.clone() }}))?;
    if result["isError"] == true {
        let message = result["content"].as_array().map(|blocks| blocks.iter()
            .filter_map(|v| v["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        bail!("Proxmox MCP {name}: {}", message.chars().take(500).collect::<String>());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_only() {
        assert_eq!(normalize_url("127.0.0.1:8811").unwrap(), "http://127.0.0.1:8811/mcp");
        assert_eq!(normalize_url("http://localhost:8811/mcp").unwrap(), "http://localhost:8811/mcp");
        for url in ["http://192.168.0.6:8811/mcp", "https://localhost:8811/mcp", "localhost:0", "localhost:8811/admin", "localhost:8811@evil/mcp"] {
            assert!(normalize_url(url).is_err(), "accepted {url}");
        }
    }
}
