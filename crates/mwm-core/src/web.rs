//! MWM's web server: the full interface in a browser plus the `/api` that
//! other MWMs use to manage this machine. Used by `mwm serve` on servers and by
//! the desktop app's "Remote access" switch (Windows / Linux / macOS). The UI files are compiled into the binary;
//! `/api/<cmd>` goes through the same `crate::api::call` as the desktop app.
//!
//! Auth: a random access token (stored in the MWM data dir, printed at start).
//! Opening `/?token=...` or submitting the login form sets an HttpOnly,
//! SameSite=Strict cookie; API calls also require a JSON content type, which a
//! cross-site form can't send - so no CSRF.

use std::io::Read;
use std::sync::Arc;

use tiny_http::{Header, Method, Request, Response, Server};

const FILES: &[(&str, &str, &str)] = &[
    ("index.html", "text/html; charset=utf-8", include_str!("../../../app/ui/index.html")),
    ("app.css", "text/css; charset=utf-8", include_str!("../../../app/ui/app.css")),
    ("app.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/app.js")),
    ("commander.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/commander.js")),
    ("toolkit.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/toolkit.js")),
    ("server.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/server.js")),
    ("fleet.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/fleet.js")),
    ("ctx.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/ctx.js")),
    ("cluster.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/cluster.js")),
    ("lab.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/lab.js")),
    ("plugins.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/plugins.js")),
    ("vendor/kdbxweb.min.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/vendor/kdbxweb.min.js")),
    ("vendor/argon2.umd.min.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/vendor/argon2.umd.min.js")),
    ("vault.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/vault.js")),
    ("demo.js", "text/javascript; charset=utf-8", include_str!("../../../app/ui/demo.js")),
    ("favicon.svg", "image/svg+xml", include_str!("../../../assets/mwm-logo.svg")),
];

pub(crate) fn random_hex(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("operating system random source unavailable");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn random_token() -> String {
    random_hex(20)
}

pub fn token(renew: bool) -> String {
    let path = crate::util::data_dir().join("web-token");
    if !renew {
        if let Ok(t) = std::fs::read_to_string(&path) {
            let t = t.trim().to_string();
            if t.len() >= 20 {
                return t;
            }
        }
    }
    let t = random_token();
    let _ = std::fs::write(&path, &t);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    t
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("valid header")
}

fn cookie_token(req: &Request) -> Option<String> {
    req.headers().iter().find(|h| h.field.equiv("Cookie")).and_then(|h| {
        h.value.as_str().split(';').find_map(|c| c.trim().strip_prefix("mwm=").map(String::from))
    })
}

fn eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn login_page(err: bool) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>MWM - sign in</title><link rel="icon" href="favicon.svg">
<style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#0e131d;color:#e8ebf3;font:15px Segoe UI,system-ui,sans-serif}}form{{background:#151b28;border:1px solid #242d40;border-radius:16px;padding:32px;width:min(380px,90vw)}}img{{width:64px}}h1{{margin:10px 0 4px;letter-spacing:.1em}}p{{color:#8e97ab;margin:0 0 18px}}input{{width:100%;box-sizing:border-box;background:#0e131d;border:1px solid #242d40;border-radius:10px;padding:11px;color:#e8ebf3;font:13px monospace}}button{{margin-top:14px;width:100%;padding:11px;border:0;border-radius:999px;background:#3ff0a4;color:#062414;font-weight:700;cursor:pointer}}.e{{color:#f26d6d;margin-top:10px}}</style></head>
<body><form method="post" action="login"><img src="favicon.svg" alt=""><h1>MWM</h1><p>Move Weight Manager on this server. Paste the access token printed by <code>mwm serve</code> (or run <code>mwm serve --show-token</code>).</p>
<input name="token" placeholder="access token" autofocus autocomplete="off"><button>Sign in</button>{}</form></body></html>"#,
        if err { r#"<div class="e">That token is not right.</div>"# } else { "" }
    )
}

fn respond(req: Request, resp: Response<std::io::Cursor<Vec<u8>>>) {
    let _ = req.respond(resp.with_header(header("X-Content-Type-Options", "nosniff")).with_header(header("Referrer-Policy", "no-referrer")));
}

fn set_cookie_redirect(req: Request, tok: &str) {
    let resp = Response::from_data(Vec::new())
        .with_status_code(303)
        .with_header(header("Location", "./"))
        .with_header(header("Set-Cookie", &format!("mwm={tok}; HttpOnly; SameSite=Strict; Path=/; Max-Age=2592000")));
    respond(req, resp);
}

fn handle(mut req: Request, tok: &str, read_only: bool) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let path = path.trim_start_matches('/');
    // Browser: cookie. Another MWM managing this one: X-MWM-Token header.
    let header_tok = req.headers().iter().find(|h| h.field.equiv("X-MWM-Token")).map(|h| h.value.as_str().to_string());
    let authed = cookie_token(&req).map(|c| eq(&c, tok)).unwrap_or(false) || header_tok.map(|h| eq(&h, tok)).unwrap_or(false);

    // Token in the link -> cookie.
    if let Some(q) = query.split('&').find_map(|kv| kv.strip_prefix("token=")) {
        if eq(q, tok) {
            return set_cookie_redirect(req, tok);
        }
    }
    if path == "login" && *req.method() == Method::Post {
        let mut body = String::new();
        let _ = req.as_reader().take(4096).read_to_string(&mut body);
        let given = body.split('&').find_map(|kv| kv.strip_prefix("token=")).unwrap_or("").trim().to_string();
        if eq(&given, tok) {
            return set_cookie_redirect(req, tok);
        }
        return respond(req, Response::from_string(login_page(true)).with_status_code(401).with_header(header("Content-Type", "text/html; charset=utf-8")));
    }
    if path == "favicon.svg" {
        return respond(req, Response::from_string(FILES.iter().find(|f| f.0 == "favicon.svg").map(|f| f.2).unwrap_or("")).with_header(header("Content-Type", "image/svg+xml")));
    }
    if !authed {
        if path.starts_with("api/") {
            return respond(req, Response::from_string("sign in first").with_status_code(401));
        }
        return respond(req, Response::from_string(login_page(false)).with_header(header("Content-Type", "text/html; charset=utf-8")));
    }

    if let Some(cmd) = path.strip_prefix("api/") {
        let json_ct = req.headers().iter().any(|h| h.field.equiv("Content-Type") && h.value.as_str().starts_with("application/json"));
        if *req.method() != Method::Post || !json_ct {
            return respond(req, Response::from_string("POST application/json only").with_status_code(400));
        }
        let mut body = String::new();
        let _ = req.as_reader().take(16 << 20).read_to_string(&mut body);
        let args: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        if cmd.starts_with("remote_access") {
            return respond(req, Response::from_string("remote access settings can only be changed on the machine itself").with_status_code(403));
        }
        if read_only && crate::api::is_mutating_call(cmd, &args) {
            return respond(req, Response::from_string("This MWM server is read-only (--read-only).").with_status_code(403));
        }
        return match crate::api::call(cmd, &args) {
            Ok(v) => respond(req, Response::from_string(v.to_string()).with_header(header("Content-Type", "application/json"))),
            Err(e) => respond(req, Response::from_string(e).with_status_code(500)),
        };
    }

    let name = if path.is_empty() { "index.html" } else { path };
    match FILES.iter().find(|f| f.0 == name) {
        Some((n, ct, body)) => {
            let body = if *n == "index.html" {
                body.replacen("<head>", "<head>\n  <meta name=\"mwm-web\" content=\"1\">\n  <link rel=\"icon\" href=\"favicon.svg\">", 1)
            } else {
                body.to_string()
            };
            respond(req, Response::from_string(body).with_header(header("Content-Type", ct)).with_header(header("Cache-Control", "no-cache")));
        }
        None => respond(req, Response::from_string("not found").with_status_code(404)),
    }
}

pub fn run(bind: &str, read_only: bool, renew: bool) -> anyhow::Result<()> {
    let server = Arc::new(Server::http(bind).map_err(|e| anyhow::anyhow!("cannot listen on {bind}: {e}"))?);
    announce(bind, read_only, &token(renew));
    serve_loop(server, read_only);
    Ok(())
}

fn announce(bind: &str, read_only: bool, tok: &str) {
    let host = if bind.starts_with("0.0.0.0") { bind.replacen("0.0.0.0", &lan_ip().unwrap_or_else(|| "this-machine".into()), 1) } else { bind.to_string() };
    println!("MWM web UI listening on http://{bind}{}", if read_only { "  (read-only)" } else { "" });
    println!("Open:  http://{host}/?token={tok}");
    println!("Token file: {}", crate::util::data_dir().join("web-token").display());
    if bind.starts_with("0.0.0.0") {
        println!("Note: plain HTTP on your LAN - put it behind a reverse proxy / VPN for anything wider.");
    }
}

fn serve_loop(server: Arc<Server>, read_only: bool) {
    let tok = Arc::new(token(false));
    for req in server.incoming_requests() {
        let tok = tok.clone();
        std::thread::spawn(move || handle(req, &tok, read_only));
    }
}

/// This machine's LAN address (the one used for outbound traffic).
pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("1.1.1.1:80").ok()?;
    s.local_addr().ok().map(|a| a.ip().to_string())
}

// ---------------------------------------------- desktop "Remote access" ----

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoteAccess {
    pub enabled: bool,
    pub bind: String,
    pub read_only: bool,
}

impl Default for RemoteAccess {
    fn default() -> Self {
        RemoteAccess { enabled: false, bind: "0.0.0.0:7777".into(), read_only: false }
    }
}

fn ra_file() -> std::path::PathBuf {
    crate::util::data_dir().join("remote-access.json")
}

fn running() -> &'static std::sync::Mutex<Option<(Arc<Server>, RemoteAccess)>> {
    static R: std::sync::OnceLock<std::sync::Mutex<Option<(Arc<Server>, RemoteAccess)>>> = std::sync::OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn ra_settings() -> RemoteAccess {
    std::fs::read_to_string(ra_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn ra_status() -> serde_json::Value {
    let st = ra_settings();
    let live = running().lock().ok().and_then(|g| g.as_ref().map(|(_, c)| c.clone()));
    let port = st.bind.rsplit(':').next().unwrap_or("7777").to_string();
    let ip = lan_ip().unwrap_or_else(|| "127.0.0.1".into());
    let tok = token(false);
    serde_json::json!({
        "enabled": st.enabled, "bind": st.bind, "read_only": st.read_only,
        "running": live.is_some(),
        "url": format!("http://{ip}:{port}"),
        "link": format!("http://{ip}:{port}/?token={tok}"),
        "token": tok,
    })
}

/// Start (or restart) the in-app server and remember the choice.
pub fn ra_start(bind: &str, read_only: bool) -> anyhow::Result<serde_json::Value> {
    ra_stop_server();
    let server = Arc::new(Server::http(bind).map_err(|e| anyhow::anyhow!("cannot listen on {bind}: {e}"))?);
    let cfg = RemoteAccess { enabled: true, bind: bind.into(), read_only };
    std::fs::write(ra_file(), serde_json::to_string_pretty(&cfg)?)?;
    let s2 = server.clone();
    std::thread::spawn(move || serve_loop(s2, read_only));
    if let Ok(mut g) = running().lock() {
        *g = Some((server, cfg));
    }
    Ok(ra_status())
}

fn ra_stop_server() {
    if let Ok(mut g) = running().lock() {
        if let Some((s, _)) = g.take() {
            s.unblock();
        }
    }
}

pub fn ra_stop() -> anyhow::Result<serde_json::Value> {
    ra_stop_server();
    let mut cfg = ra_settings();
    cfg.enabled = false;
    std::fs::write(ra_file(), serde_json::to_string_pretty(&cfg)?)?;
    Ok(ra_status())
}

pub fn ra_new_token() -> serde_json::Value {
    token(true);
    ra_status()
}

/// Called once at desktop start-up: bring the server back if it was on.
pub fn ra_autostart() {
    let cfg = ra_settings();
    if cfg.enabled {
        let _ = ra_start(&cfg.bind, cfg.read_only);
    }
}
