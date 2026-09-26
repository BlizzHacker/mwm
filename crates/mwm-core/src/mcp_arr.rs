//! Optional bridge to the upstream mcp-arr Streamable HTTP server.
//! The upstream process stays on the selected machine's loopback interface;
//! MWM's existing authenticated desktop/server API is the remote boundary.

use std::{path::PathBuf, time::Duration};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::util;

fn file() -> PathBuf { util::data_dir().join("mcp-arr.json") }

fn configured_url() -> String {
    std::fs::read_to_string(file()).ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["url"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// mcp-arr has no documented authentication for its HTTP mode. Only a local
/// listener is accepted so its media-control tools are never exposed on LAN.
fn normalize_url(raw: &str) -> Result<String> {
    let raw = raw.trim().trim_end_matches('/');
    let raw = raw.strip_prefix("http://").unwrap_or(raw);
    if raw.contains('@') || raw.contains('?') || raw.contains('#') || raw.contains('\\') {
        bail!("use a local MCP-ARR address without credentials or query parameters");
    }
    let (hostport, path) = raw.split_once('/').unwrap_or((raw, ""));
    let (host, port) = hostport.split_once(':').unwrap_or((hostport, "3000"));
    if !matches!(host, "127.0.0.1" | "localhost") || port.parse::<u16>().ok().filter(|p| *p > 0).is_none() {
        bail!("MCP-ARR must listen on this machine's loopback address (127.0.0.1:3000)");
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
                    bail!("MCP-ARR: {}", error["message"].as_str().unwrap_or("request failed"));
                }
                return Ok(v["result"].clone());
            }
        }
    }
    bail!("MCP-ARR returned no JSON-RPC reply")
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
            Err(ureq::Error::Status(code, _)) => bail!("MCP-ARR returned HTTP {code}"),
            Err(e) => bail!("cannot reach MCP-ARR: {e}"),
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
    if url.is_empty() { bail!("connect MCP-ARR on this machine first"); }
    Client::connect(&url)
}

pub fn status() -> Value {
    let url = configured_url();
    let (reachable, error) = if url.is_empty() { (false, String::new()) } else {
        match Client::connect(&url) { Ok(_) => (true, String::new()), Err(e) => (false, e.to_string()) }
    };
    json!({"configured": !url.is_empty(), "url": url, "reachable": reachable, "error": error})
}

pub fn configure(url: &str) -> Result<Value> {
    let url = normalize_url(url)?;
    Client::connect(&url).context("start MCP-ARR in HTTP mode on this machine, then connect")?;
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

pub fn call(name: &str, args: &Value) -> Result<Value> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("invalid MCP-ARR tool name");
    }
    if !args.is_object() && !args.is_null() { bail!("tool arguments must be an object"); }
    let mut client = client()?;
    let listed = client.request("tools/list", json!({}))?;
    if !listed["tools"].as_array().is_some_and(|tools| tools.iter().any(|t| t["name"] == name)) {
        bail!("MCP-ARR does not advertise tool {name}");
    }
    let result = client.request("tools/call", json!({"name":name, "arguments":if args.is_null() { json!({}) } else { args.clone() }}))?;
    if result["isError"] == true {
        let message = result["content"].as_array().map(|blocks| blocks.iter()
            .filter_map(|v| v["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        bail!("MCP-ARR {name}: {}", message.chars().take(500).collect::<String>());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_loopback_endpoints() {
        assert_eq!(normalize_url("localhost:3000").unwrap(), "http://localhost:3000/mcp");
        assert_eq!(normalize_url("http://127.0.0.1:3000/mcp").unwrap(), "http://127.0.0.1:3000/mcp");
        for url in ["http://192.168.0.1:3000/mcp", "https://example.com/mcp", "localhost:0", "localhost:3000/admin", "localhost:3000@evil/mcp"] {
            assert!(normalize_url(url).is_err(), "accepted {url}");
        }
    }
    #[test]
    fn reads_json_and_sse() {
        let msg = r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[]}}"#;
        assert!(response(msg, 2).is_ok());
        assert!(response(&format!("event: message\ndata: {msg}\n\n"), 2).is_ok());
    }
}
