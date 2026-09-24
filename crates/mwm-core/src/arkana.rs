//! Arkana integration - every user runs THEIR OWN Arkana (the MIT-licensed
//! malware-analysis lab, github.com/JameZUK/Arkana). MWM can install it on
//! this machine (Docker), start/stop/update it, and talk to it over MCP
//! (streamable HTTP + the user's own Bearer key) to run deep analysis.
//! Nothing here points at anyone else's instance.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{jobs, util};

pub const DEFAULT_REPO: &str = "https://github.com/JameZUK/Arkana.git";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// MCP endpoint, e.g. http://127.0.0.1:8082/mcp
    pub url: String,
    /// ARKANA_API_KEY (Bearer).
    pub key: String,
    /// Host folder Arkana sees as /samples.
    pub samples_dir: String,
    /// Install folder when MWM installed it.
    pub dir: String,
    pub managed: bool,
}

fn file() -> PathBuf {
    util::data_dir().join("arkana.json")
}

pub fn settings() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save(s: &Settings) -> anyhow::Result<()> {
    let p = file();
    std::fs::write(&p, serde_json::to_string_pretty(s)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn have(cmd: &str) -> bool {
    util::run_capture(cmd, &["--version"]).map(|(c, _)| c == 0).unwrap_or(false)
}

fn compose_ok() -> bool {
    util::run_capture("docker", &["compose", "version"]).map(|(c, _)| c == 0).unwrap_or(false)
}

fn default_dir() -> PathBuf {
    if cfg!(unix) && crate::sys::is_elevated() {
        PathBuf::from("/opt/arkana")
    } else {
        util::data_dir().join("arkana")
    }
}

/// What the UI needs to decide between "Install", "Connect" and "Analyze".
pub fn status() -> Value {
    let s = settings();
    let running = if s.managed && !s.dir.is_empty() {
        util::run_capture("docker", &["ps", "--filter", "name=arkana", "--format", "{{.Names}} {{.Status}}"]).map(|(_, o)| o.trim().to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let reachable = if s.url.is_empty() { false } else { McpClient::connect(&s.url, &s.key).is_ok() };
    json!({
        "configured": !s.url.is_empty(),
        "url": s.url,
        "has_key": !s.key.is_empty(),
        "samples_dir": s.samples_dir,
        "dir": s.dir,
        "managed": s.managed,
        "container": running,
        "reachable": reachable,
        "docker": have("docker"),
        "compose": compose_ok(),
        "git": have("git"),
        "platform": std::env::consts::OS,
        "default_dir": default_dir().to_string_lossy(),
    })
}

/// Point MWM at an Arkana the user already runs.
pub fn configure(url: &str, key: &str, samples_dir: &str) -> anyhow::Result<Value> {
    let mut u = url.trim().trim_end_matches('/').to_string();
    if u.is_empty() {
        bail!("enter the Arkana address, e.g. http://127.0.0.1:8082");
    }
    if !u.starts_with("http") {
        u = format!("http://{u}");
    }
    if !u.ends_with("/mcp") {
        u.push_str("/mcp");
    }
    let mut s = settings();
    let effective_key = if key.trim().is_empty() && s.url == u { s.key.clone() } else { key.trim().to_string() };
    McpClient::connect(&u, &effective_key).context("MWM could not talk to Arkana there - check the address and API key")?;
    s.url = u;
    s.key = effective_key;
    s.samples_dir = samples_dir.trim().into();
    save(&s)?;
    Ok(status())
}

fn stream(job: &jobs::Job, mut cmd: std::process::Command) -> anyhow::Result<()> {
    use std::io::{BufRead, BufReader};
    let mut child = cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()?;
    let err = child.stderr.take();
    let j2 = job.clone();
    let t = std::thread::spawn(move || {
        if let Some(e) = err {
            for l in BufReader::new(e).lines().map_while(Result::ok) {
                j2.line(l);
            }
        }
    });
    if let Some(out) = child.stdout.take() {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            job.current(&l.chars().take(120).collect::<String>());
            job.line(l);
            if job.cancelled() {
                let _ = child.kill();
                bail!("cancelled");
            }
        }
    }
    let _ = t.join();
    let st = child.wait()?;
    if !st.success() {
        bail!("step failed (exit {})", st.code().unwrap_or(-1));
    }
    Ok(())
}

fn random_key() -> String {
    // 32 hex chars from the OS RNG via the web-token generator.
    crate::web::random_hex(20)
}

/// Install the user's own Arkana with Docker. `lan` exposes it on the network
/// (still protected by the key); default is localhost only.
pub fn install(repo: &str, dir: &str, lan: bool) -> u64 {
    let repo = if repo.trim().is_empty() { DEFAULT_REPO.to_string() } else { repo.trim().to_string() };
    let dir = if dir.trim().is_empty() { default_dir() } else { PathBuf::from(dir.trim()) };
    jobs::start("arkana", "Install Arkana", "lab", move |job| {
        if !repo.starts_with("https://") || repo.contains([' ', ';', '&', '|', '`', '$']) {
            bail!("the repository must be an https:// git URL");
        }
        // Docker first - offer the distro packages on Debian/Ubuntu/Proxmox.
        if !have("docker") || !compose_ok() {
            if cfg!(target_os = "linux") && have("apt-get") && crate::sys::is_elevated() {
                job.line("Installing Docker from the distribution packages...");
                let mut c = util::cmd("sh");
                c.args(["-c", "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && (apt-get install -y -qq docker.io docker-compose-plugin || apt-get install -y -qq docker.io docker-compose-v2 || apt-get install -y -qq docker.io docker-compose) && systemctl enable --now docker"]);
                stream(job, c)?;
            } else {
                bail!("Docker with the compose plugin is required. Windows/macOS: install Docker Desktop. Linux: install docker.io and docker-compose-plugin (or run MWM as root to let it do that).");
            }
        }
        if !have("git") {
            bail!("git is required (apt install git / winget install Git.Git)");
        }
        if dir.join(".git").is_dir() {
            job.line(format!("Updating existing checkout in {}", dir.display()));
            let mut c = util::cmd("git");
            c.args(["-C", &dir.to_string_lossy(), "pull", "--ff-only"]);
            stream(job, c)?;
        } else {
            std::fs::create_dir_all(dir.parent().unwrap_or(Path::new(".")))?;
            job.line(format!("Cloning {repo} into {}", dir.display()));
            let mut c = util::cmd("git");
            c.args(["clone", "--depth", "1", &repo, &dir.to_string_lossy()]);
            stream(job, c)?;
        }
        std::fs::create_dir_all(dir.join("samples"))?;
        std::fs::create_dir_all(dir.join("output"))?;
        // This user's own key - never shared, stored only in .env and MWM's data dir.
        let key = random_key();
        let mut env = std::fs::File::create(dir.join(".env"))?;
        writeln!(env, "ARKANA_API_KEY={key}\nARKANA_PORT=8082")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.join(".env"), std::fs::Permissions::from_mode(0o600))?;
        }
        // Upstream's healthcheck sends an unauthenticated GET to /mcp. That
        // endpoint correctly rejects it with 401, marking a working service
        // unhealthy. Check the local listener instead; MCP initialize below
        // still verifies API behavior and the key before the job succeeds.
        let mut override_yaml = "services:\n  arkana-http:\n    healthcheck:\n      test: [\"CMD\", \"python\", \"-c\", \"import socket; socket.create_connection(('127.0.0.1', 8082), 3).close()\"]\n".to_string();
        if lan {
            override_yaml.push_str("    ports: !override\n      - \"0.0.0.0:8082:8082\"\n");
        }
        std::fs::write(dir.join("docker-compose.override.yml"), override_yaml)?;
        job.line("Building the Arkana image - the first build takes 10-30 minutes...");
        let mut c = util::cmd("docker");
        c.current_dir(&dir).args(["compose", "build", "arkana-http"]);
        stream(job, c)?;
        let mut c = util::cmd("docker");
        c.current_dir(&dir).args(["compose", "up", "-d", "arkana-http"]);
        stream(job, c)?;
        let url = "http://127.0.0.1:8082/mcp".to_string();
        job.line("Waiting for Arkana to answer...");
        let mut ok = false;
        for _ in 0..90 {
            if McpClient::connect(&url, &key).is_ok() {
                ok = true;
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        save(&Settings { url, key, samples_dir: dir.join("samples").to_string_lossy().into(), dir: dir.to_string_lossy().into(), managed: true })?;
        if !ok {
            bail!("Arkana started but is not answering yet - check `docker compose logs` in {}", dir.display());
        }
        Ok(format!("Arkana is running in {} (localhost:8082{})", dir.display(), if lan { ", also on your LAN" } else { "" }))
    })
}

/// start | stop | restart | update | logs for an MWM-installed Arkana.
pub fn control(action: &str) -> anyhow::Result<Value> {
    let s = settings();
    if !s.managed || s.dir.is_empty() {
        bail!("this Arkana was not installed by MWM - manage it where it runs");
    }
    let args: Vec<&str> = match action {
        "start" => vec!["compose", "up", "-d", "arkana-http"],
        "stop" => vec!["compose", "stop", "arkana-http"],
        "restart" => vec!["compose", "restart", "arkana-http"],
        "logs" => vec!["compose", "logs", "--tail", "120", "arkana-http"],
        "update" => {
            let dir = s.dir.clone();
            let id = jobs::start("arkana", "Update Arkana", "lab", move |job| {
                let mut c = util::cmd("git");
                c.args(["-C", &dir, "pull", "--ff-only"]);
                stream(job, c)?;
                let mut c = util::cmd("docker");
                c.current_dir(&dir).args(["compose", "build", "arkana-http"]);
                stream(job, c)?;
                let mut c = util::cmd("docker");
                c.current_dir(&dir).args(["compose", "up", "-d", "arkana-http"]);
                stream(job, c)?;
                Ok("Arkana updated".into())
            });
            return Ok(json!({"job": id}));
        }
        other => bail!("unknown action {other}"),
    };
    let mut c = util::cmd("docker");
    let out = c.current_dir(&s.dir).args(&args).output()?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    Ok(json!({"ok": out.status.success(), "output": text.lines().rev().take(150).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")}))
}

// ------------------------------------------------------------ MCP client ----

struct McpClient {
    agent: ureq::Agent,
    url: String,
    key: String,
    session: Option<String>,
    next: u64,
}

/// Replies may be plain JSON or an SSE stream ("data: {...}" lines).
fn parse_reply(body: &str, id: u64) -> anyhow::Result<Value> {
    let candidates: Vec<&str> = if body.trim_start().starts_with('{') { vec![body.trim()] } else { body.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim).collect() };
    for c in candidates.iter().rev() {
        if let Ok(v) = serde_json::from_str::<Value>(c) {
            if v["id"].as_u64() == Some(id) {
                if let Some(e) = v.get("error") {
                    bail!("Arkana: {}", e["message"].as_str().unwrap_or("error"));
                }
                return Ok(v["result"].clone());
            }
        }
    }
    bail!("no reply from Arkana")
}

impl McpClient {
    fn post(&mut self, msg: Value, timeout: u64) -> anyhow::Result<(String, Option<String>)> {
        let mut req = self
            .agent
            .post(&self.url)
            .timeout(Duration::from_secs(timeout))
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream");
        if !self.key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", self.key));
        }
        if let Some(s) = &self.session {
            req = req.set("Mcp-Session-Id", s);
        }
        match req.send_string(&msg.to_string()) {
            Ok(r) => {
                let sid = r.header("mcp-session-id").map(String::from);
                Ok((r.into_string().unwrap_or_default(), sid))
            }
            Err(ureq::Error::Status(401, _)) => bail!("Arkana rejected the API key"),
            Err(ureq::Error::Status(c, r)) => bail!("Arkana answered {c}: {}", r.into_string().unwrap_or_default().chars().take(200).collect::<String>()),
            Err(e) => bail!("can't reach Arkana at {}: {}", self.url, e.kind()),
        }
    }

    fn connect(url: &str, key: &str) -> anyhow::Result<Self> {
        let mut c = McpClient { agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).build(), url: url.into(), key: key.into(), session: None, next: 1 };
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "mwm", "version": crate::VERSION}}});
        let (body, sid) = c.post(init, 20)?;
        parse_reply(&body, 1)?;
        c.session = sid;
        c.next = 2;
        let _ = c.post(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), 10);
        Ok(c)
    }

    fn call(&mut self, tool: &str, args: Value, timeout: u64) -> anyhow::Result<Value> {
        let id = self.next;
        self.next += 1;
        let (body, _) = self.post(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": tool, "arguments": args}}), timeout)?;
        let r = parse_reply(&body, id)?;
        // Tool results arrive as content blocks; most Arkana tools return one JSON text block.
        let text: String = r["content"].as_array().map(|a| a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        if r["isError"].as_bool().unwrap_or(false) {
            bail!("{tool}: {}", text.chars().take(400).collect::<String>());
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    fn tools(&mut self) -> anyhow::Result<Value> {
        let id = self.next;
        self.next += 1;
        let (body, _) = self.post(json!({"jsonrpc": "2.0", "id": id, "method": "tools/list", "params": {}}), 30)?;
        let r = parse_reply(&body, id)?;
        Ok(json!(r["tools"].as_array().cloned().unwrap_or_default().iter().map(|t| json!({"name": t["name"], "description": t["description"].as_str().unwrap_or("").lines().next().unwrap_or("")})).collect::<Vec<_>>()))
    }
}

fn client() -> anyhow::Result<McpClient> {
    let s = settings();
    if s.url.is_empty() {
        bail!("Arkana is not set up on this machine - install it or connect your own in the Malware Lab");
    }
    McpClient::connect(&s.url, &s.key)
}

pub fn tools() -> anyhow::Result<Value> {
    client()?.tools()
}

/// Run any Arkana tool by name (advanced users / MCP-savvy).
pub fn tool(name: &str, args: &Value) -> anyhow::Result<Value> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        bail!("bad tool name");
    }
    client()?.call(name, if args.is_null() { json!({}) } else { args.clone() }, 900)
}

/// Copy `path` into Arkana's samples folder, open it there and pull the
/// triage report, IOCs and MITRE mapping. Runs as a job (minutes, not ms).
pub fn analyze(path: &str) -> anyhow::Result<u64> {
    let s = settings();
    if s.url.is_empty() {
        bail!("Arkana is not set up on this machine");
    }
    if s.samples_dir.is_empty() {
        bail!("tell MWM where Arkana's samples folder is (Malware Lab > Arkana > Connect)");
    }
    let src = PathBuf::from(path);
    let name = src.file_name().context("pick a file")?.to_string_lossy().to_string();
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || ".-_".contains(c) { c } else { '_' }).collect();
    if safe.is_empty() || safe == "." || safe == ".." { bail!("pick a regular file"); }
    Ok(jobs::start("arkana", &format!("Arkana: {name}"), "lab", move |job| {
        let sample_name = format!("mwm-{}-{safe}", crate::web::random_hex(8));
        let dest = Path::new(&s.samples_dir).join(&sample_name);
        std::fs::copy(&src, &dest).with_context(|| format!("copy into {}", s.samples_dir))?;
        job.line(format!("Copied to {}", dest.display()));
        let mut c = McpClient::connect(&s.url, &s.key)?;
        job.current("opening the file in Arkana");
        let opened = c.call("open_file", json!({"file_path": format!("/samples/{sample_name}")}), 900)?;
        job.line(format!("open_file: {}", opened.to_string().chars().take(300).collect::<String>()));
        job.current("triage report");
        let triage = c.call("get_triage_report", json!({"compact": true}), 900)?;
        job.line(format!("TRIAGE {}", triage));
        for (tool, args) in [("get_iocs_structured", json!({})), ("map_mitre_attack", json!({}))] {
            job.current(tool);
            match c.call(tool, args, 600) {
                Ok(v) => job.line(format!("{} {}", tool.to_uppercase(), v)),
                Err(e) => job.line(format!("{tool} skipped: {e}")),
            }
        }
        Ok(format!("Arkana analyzed {name}"))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sse_and_json_replies() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":3,\"result\":{\"ok\":1}}\n\n";
        assert_eq!(parse_reply(sse, 3).unwrap()["ok"], 1);
        let plain = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"serverInfo\":{}}}";
        assert!(parse_reply(plain, 1).is_ok());
        let err = "data: {\"jsonrpc\":\"2.0\",\"id\":2,\"error\":{\"message\":\"nope\"}}";
        assert!(parse_reply(err, 2).is_err());
        assert!(parse_reply(sse, 9).is_err());
    }
}
