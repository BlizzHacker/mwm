//! Move Weight: take heavy folders off this machine's drive and onto your own
//! server over SSH, shipping only what exists nowhere else, and keep them off.
//!
//! For every folder under the watched roots MWM works out what cannot be got
//! back from anywhere else and ships just that, verified, before the local copy
//! goes:
//!   * git repos and worktrees: a bundle of every commit and stash no remote
//!     has, the uncommitted diff, untracked files, and ignored files that are
//!     not rebuildable. Tracked files already on a remote and dependency / build
//!     folders (node_modules, target, dist, ...) are not copied at all.
//!   * any other folder: the whole folder as a gzip tar.
//!
//! Every artifact is checked on arrival (SHA-256 of what landed, and the file
//! count inside each tar) before anything local is deleted. A ledger here and a
//! RESTORE.md beside each shipment on the server say how to get it back.
//!
//! The guard (a daily scheduled task running `MoveWeightManager --guard`)
//! applies the same policy on its own: folders idle for N days move, safe
//! caches are cleaned, and the drive stops growing.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, bail, Context};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::jobs::{self, Job};
use crate::util::{cmd, data_dir, now_secs};

/// Ignored folders that are rebuilt from lockfiles / sources, never shipped.
pub const REBUILDABLE: &[&str] = &[
    "node_modules", ".pnpm-store", "target", "dist", "dist-server", ".vite", ".next", ".nuxt", ".svelte-kit",
    ".turbo", ".cache", ".parcel-cache", "coverage", "__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache",
    ".gradle", ".venv", "venv", ".tox", ".godot", ".angular", ".expo", "playwright-report", "test-results",
];

pub const GUARD_TASK: &str = "MWM Move Weight Guard";

// ------------------------------------------------------------------ policy

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Policy {
    /// `user@host` reachable with this user's SSH keys (no password prompts).
    pub server: String,
    /// Where shipments land on the server; one sub-folder per machine.
    pub remote_root: String,
    /// Folders whose children are candidates (e.g. C:\MoveWeight).
    pub roots: Vec<String>,
    /// Single folders that are candidates themselves.
    pub items: Vec<String>,
    /// Names or full paths the guard never moves.
    pub keep: Vec<String>,
    /// Run the policy on a schedule.
    pub guard: bool,
    /// The guard moves folders untouched for this many days.
    pub idle_days: u32,
    /// The guard also cleans safe temp / package caches.
    pub clean_caches: bool,
    /// The guard leaves anything whose shipment is bigger than this for review.
    pub max_ship_gb: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            server: String::new(),
            remote_root: "mwm-offload".into(),
            roots: vec![],
            items: vec![],
            keep: vec![],
            guard: false,
            idle_days: 3,
            clean_caches: true,
            max_ship_gb: 8.0,
        }
    }
}

fn policy_path() -> PathBuf {
    data_dir().join("offload-policy.json")
}

pub fn policy() -> Policy {
    fs::read_to_string(policy_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn set_policy(p: &Policy) -> anyhow::Result<Value> {
    fs::write(policy_path(), serde_json::to_vec_pretty(p)?)?;
    let installed = set_guard_task(p.guard)?;
    Ok(json!({ "saved": true, "guardInstalled": installed }))
}

// ------------------------------------------------------------------ ledger

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Moved {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub server: String,
    pub dest: String,
    pub when: u64,
    pub bytes: u64,
    pub shipped: u64,
    pub artifacts: Vec<String>,
    pub restore: String,
}

fn ledger_path() -> PathBuf {
    data_dir().join("offload-ledger.json")
}

pub fn ledger() -> Vec<Moved> {
    fs::read_to_string(ledger_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn ledger_add(m: &Moved) {
    let mut all = ledger();
    all.push(m.clone());
    let _ = fs::write(ledger_path(), serde_json::to_vec_pretty(&all).unwrap_or_default());
    // A human-readable pointer where the folder used to be.
    if let Some(parent) = Path::new(&m.path).parent() {
        let here: Vec<&Moved> = all.iter().filter(|x| Path::new(&x.path).parent() == Some(parent)).collect();
        let mut md = String::from(
            "# Moved to the server by MWM - Move Weight Manager\n\nThese folders were shipped to your server and removed from this drive.\nEach shipment has a RESTORE.md. Work on them on the server, not here.\n\n| Folder | Moved | Size here | Shipped | Server location |\n|---|---|---|---|---|\n",
        );
        for x in here.iter().rev() {
            md.push_str(&format!(
                "| {} | {} | {} | {} | `{}:{}` |\n",
                x.name,
                ymd(x.when),
                human(x.bytes),
                human(x.shipped),
                x.server,
                x.dest
            ));
        }
        let _ = fs::write(parent.join("MOVED-TO-SERVER.md"), md);
    }
}

// ------------------------------------------------------------------ scan

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    pub name: String,
    /// repo | worktree | folder
    pub kind: String,
    pub bytes: u64,
    pub files: u64,
    /// Bytes that must travel (what exists nowhere else), estimated.
    pub ship_bytes: u64,
    /// Rebuildable bytes that are simply dropped.
    pub drop_bytes: u64,
    pub idle_days: u64,
    pub unpushed: u64,
    pub stashes: u64,
    pub dirty: u64,
    pub remotes: Vec<String>,
    pub parent: String,
    pub worktrees: Vec<String>,
    /// safe | review | keep
    pub verdict: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub when: u64,
    pub items: Vec<Item>,
}

fn last_scan() -> &'static Mutex<Option<Scan>> {
    static S: OnceLock<Mutex<Option<Scan>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(None))
}

/// Every candidate folder named by the policy.
fn candidates(p: &Policy) -> Vec<PathBuf> {
    let mut out = Vec::new();
    // A watched folder inside another one is looked into, not moved whole.
    let roots: HashSet<String> = p.roots.iter().map(|r| norm_path(r)).collect();
    for r in &p.roots {
        if let Ok(rd) = fs::read_dir(r) {
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() && !ft.is_symlink() && !roots.contains(&norm_path(&e.path().to_string_lossy())) {
                    out.push(e.path());
                }
            }
        }
    }
    for i in &p.items {
        let pb = PathBuf::from(i);
        if pb.is_dir() {
            out.push(pb);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = cmd("git").arg("-C").arg(dir).args(args).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn count_lines(s: Option<String>) -> u64 {
    s.map(|t| t.lines().filter(|l| !l.trim().is_empty()).count() as u64).unwrap_or(0)
}

/// True when any component of `rel` is a rebuildable folder name. tar's
/// `--exclude` matches every path component, so such paths never reach an
/// archive and must not be listed or counted as if they would.
fn is_rebuildable(rel: &str) -> bool {
    rel.split(['/', '\\']).any(|n| REBUILDABLE.contains(&n))
}

/// Size of a path (file or folder), never following links.
fn size_of(p: &Path) -> (u64, u64) {
    let mut b = 0;
    let mut f = 0;
    for e in WalkDir::new(p).follow_links(false).into_iter().flatten() {
        if !e.file_type().is_dir() {
            if let Ok(m) = e.metadata() {
                b += m.len();
            }
            f += 1;
        }
    }
    (b, f)
}

/// Days since anything at the top of the folder (or its git state) changed.
fn idle_days(p: &Path) -> u64 {
    let mut newest = fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut probe: Vec<PathBuf> = fs::read_dir(p).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    for g in [".git/HEAD", ".git/index", ".git/logs/HEAD", ".git/FETCH_HEAD"] {
        probe.push(p.join(g));
    }
    for q in probe {
        if let Ok(t) = fs::symlink_metadata(&q).and_then(|m| m.modified()) {
            if newest.map(|n| t > n).unwrap_or(true) {
                newest = Some(t);
            }
        }
    }
    newest
        .and_then(|t| std::time::SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

/// The repo a linked worktree belongs to (its `.git` file names it).
fn worktree_parent(p: &Path) -> Option<PathBuf> {
    let t = fs::read_to_string(p.join(".git")).ok()?;
    let gitdir = t.trim().strip_prefix("gitdir:")?.trim();
    let gd = p.join(gitdir);
    // A worktree whose admin folder was pruned is no longer one: every git
    // command in it fails ("gitfile does not point to a valid repository").
    if !gd.is_dir() {
        return None;
    }
    // <repo>/.git/worktrees/<name>
    let dotgit = gd.parent()?.parent()?;
    let repo = dotgit.parent()?;
    repo.join(".git").is_dir().then(|| repo.to_path_buf())
}

/// Untracked (not ignored) files and the ignored entries worth keeping.
fn keep_lists(dir: &Path) -> (Vec<String>, Vec<String>, u64) {
    let mut untracked = Vec::new();
    let mut ignored_keep = Vec::new();
    let mut dropped = 0u64;
    // Untracked files inside a rebuildable folder (.pytest_cache/README.md,
    // node_modules.x/pkg/coverage/...) are dropped like the folder itself.
    for f in git(dir, &["ls-files", "-z", "--others", "--exclude-standard"])
        .map(|s| s.split('\0').filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        if is_rebuildable(&f) {
            dropped += fs::symlink_metadata(dir.join(&f)).map(|m| m.len()).unwrap_or(0);
        } else {
            untracked.push(f);
        }
    }
    for entry in git(dir, &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"])
        .map(|s| s.split('\0').filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>())
        .unwrap_or_default()
    {
        if is_rebuildable(&entry) {
            dropped += size_of(&dir.join(&entry)).0;
        } else {
            ignored_keep.push(entry);
        }
    }
    (untracked, ignored_keep, dropped)
}

fn keep_listed(p: &Policy, path: &Path) -> bool {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let full = path.to_string_lossy().to_lowercase();
    p.keep.iter().any(|k| {
        let k = k.to_lowercase();
        k == name || k == full
    })
}

pub fn inspect(p: &Policy, path: &Path) -> Item {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let (bytes, files) = size_of(path);
    let mut it = Item {
        path: path.to_string_lossy().to_string(),
        name,
        kind: "folder".into(),
        bytes,
        files,
        idle_days: idle_days(path),
        ..Default::default()
    };
    let dotgit = path.join(".git");
    let bare = !dotgit.exists()
        && path.join("HEAD").is_file()
        && path.join("objects").is_dir()
        && git(path, &["rev-parse", "--is-bare-repository"]).map(|s| s.trim() == "true").unwrap_or(false);
    if REBUILDABLE.contains(&it.name.as_str()) {
        // A stray dependency / build folder: nothing in it needs to travel.
        it.kind = "rebuildable".into();
        it.drop_bytes = it.bytes;
        it.note = "Rebuildable dependency or build folder: deleted, not shipped.".into();
    } else if bare {
        it.kind = "bare".into();
        it.remotes = git(path, &["remote"]).map(|s| s.lines().map(String::from).collect()).unwrap_or_default();
        it.unpushed = git(path, &["rev-list", "--count", "--branches", "--tags", "--not", "--remotes"])
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        it.ship_bytes = if it.remotes.is_empty() {
            it.bytes
        } else if it.unpushed > 0 {
            1 << 20
        } else {
            0
        };
        if it.remotes.is_empty() {
            it.note = "Bare repo with no remote: the full history is shipped.".into();
        }
    } else if dotgit.is_dir() || dotgit.is_file() {
        let worktree = dotgit.is_file();
        let parent = if worktree { worktree_parent(path) } else { None };
        if worktree && parent.is_none() {
            it.note = "Worktree whose repo is gone: shipped as a plain folder.".into();
        } else {
            it.kind = if worktree { "worktree".into() } else { "repo".into() };
            it.parent = parent.map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            it.remotes = git(path, &["remote"]).map(|s| s.lines().map(String::from).collect()).unwrap_or_default();
            it.unpushed = git(
                path,
                if worktree { &["rev-list", "--count", "HEAD", "--not", "--remotes"][..] } else { &["rev-list", "--count", "--branches", "--tags", "HEAD", "--not", "--remotes"][..] },
            )
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
            it.stashes = count_lines(git(path, &["stash", "list"]));
            it.dirty = count_lines(git(path, &["status", "--porcelain"]));
            if !worktree {
                let me = fs::canonicalize(path).ok();
                it.worktrees = git(path, &["worktree", "list", "--porcelain"])
                    .map(|s| {
                        s.lines()
                            .filter_map(|l| l.strip_prefix("worktree "))
                            .map(PathBuf::from)
                            .filter(|w| fs::canonicalize(w).ok() != me && w.exists())
                            .map(|w| w.to_string_lossy().to_string())
                            .collect()
                    })
                    .unwrap_or_default();
            }
            let (untracked, ignored_keep, dropped) = keep_lists(path);
            let mut ship: u64 = untracked.iter().map(|f| fs::metadata(path.join(f)).map(|m| m.len()).unwrap_or(0)).sum();
            ship += ignored_keep.iter().map(|f| size_of(&path.join(f)).0).sum::<u64>();
            if it.remotes.is_empty() && !worktree {
                // No remote at all: the whole history travels.
                ship += size_of(&dotgit).0;
                it.note = "No remote: the full history is shipped.".into();
            } else if it.unpushed > 0 || it.stashes > 0 {
                ship += 1 << 20; // bundles of a few commits are small
            }
            it.ship_bytes = ship;
            it.drop_bytes = dropped;
        }
    }
    if it.kind == "folder" {
        it.ship_bytes = it.bytes;
    }
    if it.kind == "rebuildable" {
        it.ship_bytes = 0;
    }
    it.verdict = if keep_listed(p, path) {
        "keep".into()
    } else if it.kind == "repo" && !it.worktrees.is_empty() {
        if it.note.is_empty() {
            it.note = format!("{} worktree(s) still use this repo; they move first.", it.worktrees.len());
        }
        "review".into()
    } else if (it.ship_bytes as f64) > p.max_ship_gb * 1_073_741_824.0 {
        "review".into()
    } else {
        "safe".into()
    };
    it
}

pub fn scan_now(p: &Policy, job: Option<&Job>) -> Scan {
    let paths = candidates(p);
    if let Some(j) = job {
        j.set_total(0, paths.len() as u64);
    }
    let mut items = Vec::new();
    for path in paths {
        if job.map(|j| j.cancelled()).unwrap_or(false) {
            break;
        }
        if let Some(j) = job {
            j.current(&path.to_string_lossy());
        }
        items.push(inspect(p, &path));
        if let Some(j) = job {
            j.progress(0, 1, "");
        }
    }
    items.sort_by_key(|a| std::cmp::Reverse(a.bytes));
    let s = Scan { when: now_secs(), items };
    if let Ok(mut g) = last_scan().lock() {
        *g = Some(s.clone());
    }
    let _ = fs::write(data_dir().join("offload-scan.json"), serde_json::to_vec(&s).unwrap_or_default());
    s
}

pub fn scan_job() -> u64 {
    let p = policy();
    jobs::start("scan", "Measuring what can move", "offload", move |job| {
        let s = scan_now(&p, Some(job));
        let total: u64 = s.items.iter().map(|i| i.bytes).sum();
        Ok(format!("{} folders, {} on this drive", s.items.len(), human(total)))
    })
}

fn stored_scan() -> Option<Scan> {
    if let Ok(g) = last_scan().lock() {
        if g.is_some() {
            return g.clone();
        }
    }
    let t = fs::read_to_string(data_dir().join("offload-scan.json")).ok()?;
    let v: Value = serde_json::from_str(&t).ok()?;
    Some(Scan {
        when: v.get("when").and_then(|x| x.as_u64()).unwrap_or(0),
        items: serde_json::from_value(v.get("items").cloned().unwrap_or(Value::Null)).unwrap_or_default(),
    })
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
        let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
        let l = |k: &str| {
            v.get(k)
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|y| y.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        Ok(Item {
            path: s("path"),
            name: s("name"),
            kind: s("kind"),
            bytes: n("bytes"),
            files: n("files"),
            ship_bytes: n("shipBytes"),
            drop_bytes: n("dropBytes"),
            idle_days: n("idleDays"),
            unpushed: n("unpushed"),
            stashes: n("stashes"),
            dirty: n("dirty"),
            remotes: l("remotes"),
            parent: s("parent"),
            worktrees: l("worktrees"),
            verdict: s("verdict"),
            note: s("note"),
        })
    }
}

// ------------------------------------------------------------------ status

pub fn status() -> Value {
    let p = policy();
    let drive = drive_of(p.roots.first().map(String::as_str).unwrap_or(""));
    json!({
        "policy": p,
        "scan": stored_scan(),
        "ledger": ledger().into_iter().rev().take(300).collect::<Vec<_>>(),
        "guardInstalled": guard_installed(),
        "guardLog": fs::read_to_string(data_dir().join("offload-guard.log")).map(|t| tail(&t, 60)).unwrap_or_default(),
        "drive": drive,
    })
}

fn drive_of(path: &str) -> Value {
    let info = crate::sys::info();
    let v = serde_json::to_value(&info).unwrap_or(Value::Null);
    let want = path.chars().next().map(|c| c.to_ascii_uppercase());
    v.get("disks")
        .and_then(|d| d.as_array())
        .and_then(|a| {
            a.iter()
                .find(|d| {
                    let m = d.get("mount").and_then(|x| x.as_str()).unwrap_or_default();
                    want.map(|w| m.to_ascii_uppercase().starts_with(w)).unwrap_or(false)
                })
                .or_else(|| a.first())
                .cloned()
        })
        .unwrap_or(Value::Null)
}

fn tail(t: &str, n: usize) -> String {
    let lines: Vec<&str> = t.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

// ------------------------------------------------------------------ server

fn ssh_bin() -> String {
    if cfg!(windows) {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let p = format!(r"{sys}\System32\OpenSSH\ssh.exe");
        if Path::new(&p).exists() {
            return p;
        }
    }
    "ssh".into()
}

fn tar_bin() -> String {
    if cfg!(windows) {
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let p = format!(r"{sys}\System32\tar.exe");
        if Path::new(&p).exists() {
            return p;
        }
    }
    "tar".into()
}

fn ssh_args(server: &str) -> Vec<String> {
    ["-o", "BatchMode=yes", "-o", "ConnectTimeout=15", "-o", "ServerAliveInterval=30", "-o", "ServerAliveCountMax=6"]
        .iter()
        .map(|s| s.to_string())
        .chain([server.to_string()])
        .collect()
}

/// Run a shell command on the server and return its output.
fn remote(server: &str, script: &str) -> anyhow::Result<String> {
    let out = cmd(&ssh_bin()).args(ssh_args(server)).arg(script).stdin(Stdio::null()).output()?;
    if !out.status.success() {
        bail!("server: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn server_check() -> Value {
    let p = policy();
    if p.server.is_empty() {
        return json!({ "ok": false, "message": "Set your server first (user@host)." });
    }
    match remote(&p.server, &format!("mkdir -p {r} && df -B1 --output=avail {r} | tail -1", r = q(&p.remote_root))) {
        Ok(out) => {
            let free: u64 = out.lines().next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
            json!({ "ok": true, "freeBytes": free, "message": format!("{} free on the server", human(free)) })
        }
        Err(e) => json!({ "ok": false, "message": e.to_string() }),
    }
}

// ------------------------------------------------------------------ ship

/// Forwards writes to the ssh pipe, hashing and counting what actually travels.
struct Wire {
    inner: ChildStdin,
    hasher: Sha256,
    sent: u64,
}

impl Write for Wire {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(b)?;
        self.hasher.update(&b[..n]);
        self.sent += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Pipe `producer`'s stdout to `<dest>/<file>` on the server (gzipped on the
/// way when `gzip`), hashing what travels. Returns (bytes sent, sha256) once
/// the server holds exactly those bytes. Fails when either side fails.
fn ship_stream(server: &str, dest: &str, file: &str, mut producer: Child, job: &Job, gzip: bool) -> anyhow::Result<(u64, String)> {
    let target = format!("{dest}/{file}");
    let mut sink = cmd(&ssh_bin())
        .args(ssh_args(server))
        .arg(format!("cat > {}", q(&format!("{target}.part"))))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("start ssh")?;
    let mut out = producer.stdout.take().ok_or_else(|| anyhow!("no producer output"))?;
    let inp = sink.stdin.take().ok_or_else(|| anyhow!("no ssh input"))?;
    let mut wire = Wire { inner: inp, hasher: Sha256::new(), sent: 0 };
    let mut buf = vec![0u8; 1 << 20];
    let pumped: anyhow::Result<()> = (|| {
        let mut enc = if gzip { Enc::Gz(GzEncoder::new(&mut wire, Compression::new(6))) } else { Enc::Raw(&mut wire) };
        loop {
            if job.cancelled() {
                bail!("cancelled");
            }
            let n = out.read(&mut buf)?;
            if n == 0 {
                break;
            }
            enc.write_all(&buf[..n]).context("sending to the server")?;
            job.progress(n as u64, 0, "");
        }
        enc.finish().context("sending to the server")?;
        Ok(())
    })();
    if let Err(e) = pumped {
        let _ = producer.kill();
        let _ = sink.kill();
        return Err(e);
    }
    let Wire { inner, hasher, sent } = wire;
    drop(inner);
    let pst = producer.wait_with_output()?;
    let sst = sink.wait_with_output()?;
    if !pst.status.success() {
        bail!("{file}: {}", tool_error(&pst.stderr, "local packing failed"));
    }
    if !sst.status.success() {
        bail!("{file}: {}", tool_error(&sst.stderr, "the server stopped receiving"));
    }
    let sha = format!("{:x}", hasher.finalize());
    let remote_sha = remote(server, &format!("sha256sum {} | cut -d' ' -f1", q(&format!("{target}.part"))))?;
    if remote_sha.trim() != sha {
        bail!("{file}: checksum on the server does not match");
    }
    remote(server, &format!("mv -f {} {}", q(&format!("{target}.part")), q(&target)))?;
    Ok((sent, sha))
}

/// Where the producer's bytes go: through gzip, or straight onto the wire.
enum Enc<'a> {
    Gz(GzEncoder<&'a mut Wire>),
    Raw(&'a mut Wire),
}

impl Write for Enc<'_> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        match self {
            Enc::Gz(e) => e.write(b),
            Enc::Raw(w) => w.write(b),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Enc::Gz(e) => e.flush(),
            Enc::Raw(w) => w.flush(),
        }
    }
}

impl Enc<'_> {
    /// Write the gzip trailer (if any) and flush.
    fn finish(self) -> std::io::Result<()> {
        match self {
            Enc::Gz(e) => e.finish().and_then(|w| w.flush()),
            Enc::Raw(w) => w.flush(),
        }
    }
}

fn producer(program: &str, args: &[String], dir: &Path, stdin_data: Option<Vec<u8>>) -> anyhow::Result<Child> {
    let mut c = cmd(program);
    c.args(args).current_dir(dir).stdout(Stdio::piped()).stderr(Stdio::piped());
    c.stdin(if stdin_data.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = c.spawn().with_context(|| format!("start {program}"))?;
    if let Some(data) = stdin_data {
        let mut sin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        std::thread::spawn(move || {
            let _ = sin.write_all(&data);
        });
    }
    Ok(child)
}

fn tar_args(extra: &[&str]) -> Vec<String> {
    let mut a: Vec<String> = vec!["-cf".into(), "-".into()];
    for r in REBUILDABLE {
        if !extra.contains(&"--no-rebuild-excludes") {
            a.push("--exclude".into());
            a.push((*r).into());
        }
    }
    a.extend(extra.iter().filter(|x| **x != "--no-rebuild-excludes").map(|s| s.to_string()));
    a
}

/// What went wrong, from a tool's stderr. tar ends every failure with "Error
/// exit delayed from previous errors", which hides the line that matters.
fn tool_error(stderr: &[u8], fallback: &str) -> String {
    let t = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty() && !l.contains("Error exit delayed")).collect();
    match lines.len() {
        0 => fallback.to_string(),
        1..=3 => lines.join("; "),
        n => format!("{} (+{} more)", lines[..3].join("; "), n - 3),
    }
}

/// The files a tar of `entries` (relative to `dir`) will hold, and a
/// fingerprint of them: every path, size and modification time. Rebuildable
/// paths are left out the way tar's `--exclude` leaves them out (any path
/// component), so the count matches what arrives on the server.
fn manifest(dir: &Path, entries: &[String], exclude_rebuildable: bool) -> (u64, String) {
    let mut n = 0;
    let mut h = Sha256::new();
    for e in entries {
        let walk = WalkDir::new(dir.join(e)).follow_links(false).sort_by_file_name().into_iter().filter_entry(|x| {
            !(exclude_rebuildable && x.file_type().is_dir() && REBUILDABLE.contains(&x.file_name().to_string_lossy().as_ref()))
        });
        for w in walk.flatten() {
            if w.file_type().is_dir() {
                continue;
            }
            let rel = w.path().strip_prefix(dir).unwrap_or(w.path()).to_string_lossy().replace('\\', "/");
            if exclude_rebuildable && is_rebuildable(&rel) {
                continue;
            }
            let m = w.metadata().ok();
            let len = m.as_ref().map(|m| m.len()).unwrap_or(0);
            let t = m.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
            h.update(format!("{rel}\0{len}\0{t}\n").as_bytes());
            n += 1;
        }
    }
    (n, format!("{:x}", h.finalize()))
}

/// Split off the entries tar cannot read: links (symlinks, junctions) whose
/// target is gone, such as a junction into a drive that is no longer there.
/// tar fails the whole archive on one ("Cannot stat"), so they are left out and
/// listed in RESTORE.md as `path -> target` instead.
fn split_dangling(dir: &Path, list: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut keep = Vec::new();
    let mut gone = Vec::new();
    for e in list {
        let p = dir.join(&e);
        if fs::symlink_metadata(&p).is_ok() && fs::metadata(&p).is_err() {
            let target = fs::read_link(&p).map(|t| t.to_string_lossy().to_string()).unwrap_or_else(|_| "?".into());
            gone.push(format!("{e} -> {target}"));
        } else {
            keep.push(e);
        }
    }
    (keep, gone)
}

/// SHA-256 of strings, each ended by a NUL so their boundaries count.
fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for x in parts {
        h.update(x.as_bytes());
        h.update([0u8]);
    }
    format!("{:x}", h.finalize())
}

/// SHA-256 and length of what a command prints, read through without keeping it.
fn digest_output(program: &str, args: &[String], dir: &Path) -> anyhow::Result<(String, u64)> {
    let mut child = producer(program, args, dir, None)?;
    let mut out = child.stdout.take().ok_or_else(|| anyhow!("no {program} output"))?;
    let mut h = Sha256::new();
    let mut n = 0u64;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let k = out.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    let st = child.wait_with_output()?;
    if !st.status.success() {
        bail!("{program}: {}", tool_error(&st.stderr, "failed"));
    }
    Ok((format!("{:x}", h.finalize()), n))
}

/// What a history bundle of `args` is made from: the refs it starts at and the
/// remote refs it stops at. Same refs, same bundle.
fn refs_fingerprint(path: &Path, kind: &str, args: &[String]) -> String {
    let mut fe = vec!["for-each-ref", "--format=%(objectname) %(refname)", "refs/remotes"];
    if kind != "worktree" {
        fe.extend(["refs/heads", "refs/tags", "refs/stash"]);
    }
    let refs = git(path, &fe).unwrap_or_default();
    let head = git(path, &["rev-parse", "HEAD"]).unwrap_or_default();
    digest(&[&args.join(" "), &refs, &head])
}

fn remote_tar_count(server: &str, path: &str) -> anyhow::Result<u64> {
    let out = remote(server, &format!("tar -tzf {f} > /dev/null && tar -tzf {f} | grep -vc '/$'; true", f = q(path)))?;
    if out.trim().is_empty() {
        bail!("the archive on the server does not read cleanly");
    }
    Ok(out.trim().parse().unwrap_or(0))
}

struct Shipped {
    dest: String,
    shipped: u64,
    artifacts: Vec<String>,
    restore: String,
    /// Nothing changed since the last verified shipment, so nothing travelled.
    unchanged: bool,
}

fn safe_name(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect()
}

fn host() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "this-pc".into())
}

// ------------------------------------------------------------------ change detection

/// One file of a verified shipment, with the fingerprint of the local state it
/// was made from. `at` is where it lives on the server: in this shipment's
/// folder, or in an earlier one when nothing changed. Empty when the part came
/// out empty (git refuses an empty bundle) and nothing was stored.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Part {
    pub file: String,
    pub fp: String,
    pub at: String,
    pub sha: String,
    pub bytes: u64,
    pub files: u64,
}

/// The last verified shipment of a folder. The guard retries a folder every day
/// until it is gone from this drive, so what the server already holds is
/// pointed at instead of shipped again.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Sent {
    pub path: String,
    pub server: String,
    pub dest: String,
    pub when: u64,
    pub parts: Vec<Part>,
    pub restore: String,
}

fn sent_path() -> PathBuf {
    data_dir().join("offload-sent.json")
}

fn sent_all() -> Vec<Sent> {
    fs::read_to_string(sent_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn last_sent(server: &str, path: &str) -> Option<Sent> {
    let key = norm_path(path);
    sent_all().into_iter().find(|s| s.server == server && norm_path(&s.path) == key)
}

fn sent_record(s: &Sent) {
    let key = norm_path(&s.path);
    let mut all: Vec<Sent> = sent_all().into_iter().filter(|x| !(x.server == s.server && norm_path(&x.path) == key)).collect();
    all.push(s.clone());
    let _ = fs::write(sent_path(), serde_json::to_vec_pretty(&all).unwrap_or_default());
}

/// The earlier copy of `file` that stands in for a new one: same fingerprint.
fn reusable<'a>(prev: Option<&'a Sent>, file: &str, fp: &str) -> Option<&'a Part> {
    prev?.parts.iter().find(|x| x.file == file && x.fp == fp)
}

/// True when the folder is exactly as it was at the last shipment: the same
/// parts, each with the same fingerprint.
fn unchanged(prev: &Sent, wanted: &[(String, String)]) -> bool {
    prev.parts.len() == wanted.len() && wanted.iter().all(|(f, fp)| reusable(Some(prev), f, fp).is_some())
}

/// `prev` without the parts whose file is no longer on the server at its size
/// (cleaned up by hand, say), so those are shipped again.
fn still_there(server: &str, mut prev: Sent) -> anyhow::Result<Sent> {
    let paths: Vec<String> = prev.parts.iter().filter(|x| !x.at.is_empty()).map(|x| q(&x.at)).collect();
    if paths.is_empty() {
        return Ok(prev);
    }
    let out = remote(server, &format!("stat -c '%s %n' {} 2>/dev/null; true", paths.join(" ")))?;
    let sizes = parse_sizes(&out);
    prev.parts.retain(|x| x.at.is_empty() || sizes.iter().any(|(n, f)| *f == x.at && *n == x.bytes));
    Ok(prev)
}

/// `stat -c '%s %n'` output as (size, path).
fn parse_sizes(out: &str) -> Vec<(u64, &str)> {
    out.lines()
        .filter_map(|l| {
            let (n, f) = l.split_once(' ')?;
            Some((n.parse().ok()?, f))
        })
        .collect()
}

fn artifact_line(x: &Part, dest: &str) -> String {
    let files = if x.files > 0 { format!(" ({} files)", x.files) } else { String::new() };
    if x.at == format!("{dest}/{}", x.file) {
        format!("{} {}{files}", x.file, x.sha)
    } else {
        format!("{} {}{files} - unchanged, kept at {}", x.file, x.sha, x.at)
    }
}

/// A fresh folder for this shipment, `<root>/<host>/<yyyymmdd>-<name>`, with
/// the time added when today's already exists: a second run the same day never
/// writes into the first one's folder.
fn new_dest(p: &Policy, it: &Item) -> anyhow::Result<String> {
    let now = now_secs();
    let base = format!("{}/{}/{}-{}", p.remote_root.trim_end_matches('/'), safe_name(&host().to_lowercase()), ymd(now), safe_name(&it.name));
    let alt = format!("{base}-{}", hms(now));
    let out = remote(&p.server, &format!("d={}; [ -e \"$d\" ] && d={}; mkdir -p \"$(dirname \"$d\")\" && mkdir \"$d\" && printf %s \"$d\"", q(&base), q(&alt)))?;
    Ok(out.trim().to_string())
}

fn bundle_args(it: &Item) -> Vec<String> {
    let mut args: Vec<String> = vec!["-C".into(), it.path.clone(), "bundle".into(), "create".into(), "-".into()];
    if it.kind == "worktree" {
        args.push("HEAD".into());
    } else if it.kind == "bare" {
        args.extend(["--branches".into(), "--tags".into()]);
    } else {
        args.extend(["--branches".into(), "--tags".into(), "HEAD".into()]);
        if it.stashes > 0 {
            args.push("refs/stash".into());
        }
    }
    if !it.remotes.is_empty() {
        args.extend(["--not".into(), "--remotes".into()]);
    }
    args
}

/// Ship one folder. Every part is measured here first (a fingerprint of what it
/// would hold); a folder that is exactly as it was at its last verified
/// shipment ships nothing, and an unchanged part of a changed folder is pointed
/// at (PARTS.json, RESTORE.md) instead of sent again. A shipment that fails is
/// removed from the server, so no `.part` files or half folders pile up.
fn ship_item(p: &Policy, it: &Item, job: &Job) -> anyhow::Result<Shipped> {
    let path = PathBuf::from(&it.path);
    let parent_dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();

    job.current(&format!("{}: checking what changed", it.name));
    let mut wanted: Vec<(String, String)> = Vec::new();
    let mut folder_files = 0u64;
    let folder_args = tar_args(&["--no-rebuild-excludes", "-C", &parent_dir.to_string_lossy(), &it.name]);
    let mut meta = Value::Null;
    let mut meta_bytes = Vec::new();
    let mut bargs: Vec<String> = Vec::new();
    let mut list: Vec<String> = Vec::new();
    let mut dangling: Vec<String> = Vec::new();
    let mut list_files = 0u64;
    let untracked_args = tar_args(&["--null", "-T", "-"]);
    if it.kind == "folder" {
        let (n, fp) = manifest(&parent_dir, std::slice::from_ref(&it.name), false);
        folder_files = n;
        wanted.push(("folder.tar.gz".into(), digest(&[&folder_args.join(" "), &fp])));
    } else if it.kind != "rebuildable" {
        // Remotes, branches and HEAD, so the tree can be re-cloned exactly.
        meta = json!({
            "remotes": git(&path, &["remote", "-v"]).unwrap_or_default(),
            "branches": git(&path, &["for-each-ref", "--format=%(refname:short) %(objectname) %(upstream:short)", "refs/heads"]).unwrap_or_default(),
            "head": git(&path, &["rev-parse", "HEAD"]).unwrap_or_default().trim(),
            "headRef": git(&path, &["symbolic-ref", "-q", "--short", "HEAD"]).unwrap_or_default().trim(),
            "shallow": git(&path, &["rev-parse", "--is-shallow-repository"]).unwrap_or_default().trim() == "true",
            "kind": it.kind, "parent": it.parent, "path": it.path,
        });
        meta_bytes = serde_json::to_vec_pretty(&meta)?;
        wanted.push(("git-state.json".into(), format!("{:x}", Sha256::digest(&meta_bytes))));
        // Every commit and stash no remote has.
        if it.unpushed > 0 || it.stashes > 0 || it.remotes.is_empty() {
            bargs = bundle_args(it);
            wanted.push(("unpushed.bundle".into(), refs_fingerprint(&path, &it.kind, &bargs)));
        }
        // Uncommitted changes to tracked files.
        if it.dirty > 0 {
            let (fp, n) = digest_output("git", &patch_args(it), &path)?;
            if n > 0 {
                wanted.push(("uncommitted.patch".into(), fp));
            }
        }
        // Untracked files, and ignored files that are not rebuildable.
        if it.kind != "bare" {
            let (untracked, ignored_keep, _) = keep_lists(&path);
            let all: Vec<String> = untracked.into_iter().chain(ignored_keep).map(|x| x.trim_end_matches('/').to_string()).collect();
            (list, dangling) = split_dangling(&path, all);
            if !list.is_empty() {
                let (n, fp) = manifest(&path, &list, true);
                list_files = n;
                wanted.push(("untracked.tar.gz".into(), digest(&[&untracked_args.join(" "), &fp])));
            }
        }
    }

    let prev = match last_sent(&p.server, &it.path) {
        Some(s) => Some(still_there(&p.server, s)?),
        None => None,
    };
    if let Some(prev) = prev.as_ref().filter(|s| unchanged(s, &wanted)) {
        return Ok(Shipped {
            dest: prev.dest.clone(),
            shipped: 0,
            artifacts: prev.parts.iter().filter(|x| !x.at.is_empty()).map(|x| artifact_line(x, &prev.dest)).collect(),
            restore: prev.restore.clone(),
            unchanged: true,
        });
    }

    let dest = new_dest(p, it)?;
    let mut parts: Vec<Part> = Vec::new();
    let mut shipped = 0u64;
    let fp_of = |file: &str| wanted.iter().find(|(f, _)| f == file).map(|(_, fp)| fp.clone()).unwrap_or_default();
    let prev = prev.as_ref();
    let sent: anyhow::Result<String> = (|| {
        let here = |file: &str| format!("{dest}/{file}");
        // Reuse the earlier copy when the fingerprint matches; else run `ship`.
        type Ship<'s> = &'s mut dyn FnMut() -> anyhow::Result<Option<(u64, String)>>;
        let part = |parts: &mut Vec<Part>, shipped: &mut u64, file: &str, files: u64, ship: Ship| -> anyhow::Result<()> {
            let fp = fp_of(file);
            if let Some(old) = reusable(prev, file, &fp) {
                parts.push(old.clone());
                return Ok(());
            }
            let (at, bytes, sha) = match ship()? {
                Some((n, sha)) => (here(file), n, sha),
                None => (String::new(), 0, String::new()),
            };
            *shipped += bytes;
            parts.push(Part { file: file.into(), fp, at, sha, bytes, files });
            Ok(())
        };
        let mut restore = format!("# {}\n\nShipped by MWM from `{}` on {} ({}).\n\n", it.name, it.path, host(), ymd(now_secs()));

        if it.kind == "rebuildable" {
            restore.push_str("A rebuildable dependency / build folder. Nothing was shipped: reinstall or rebuild to get it back.\n");
        } else if it.kind == "folder" {
            part(&mut parts, &mut shipped, "folder.tar.gz", folder_files, &mut || {
                job.current(&format!("{}: packing the folder", it.name));
                let prod = producer(&tar_bin(), &folder_args, &parent_dir, None)?;
                let (n, sha) = ship_stream(&p.server, &dest, "folder.tar.gz", prod, job, true)?;
                let there = remote_tar_count(&p.server, &here("folder.tar.gz"))?;
                if there != folder_files {
                    bail!("{}: {} files packed, {} arrived", it.name, folder_files, there);
                }
                Ok(Some((n, sha)))
            })?;
            restore.push_str(&format!("Restore on the server:\n\n    tar -xzf {} -C /where/you/want\n", at(&parts, "folder.tar.gz", &dest)));
        } else {
            let (n, sha) = ship_bytes(&p.server, &dest, "git-state.json", meta_bytes.clone(), job)?;
            shipped += n;
            parts.push(Part { file: "git-state.json".into(), fp: fp_of("git-state.json"), at: here("git-state.json"), sha, bytes: n, files: 0 });

            if !bargs.is_empty() {
                part(&mut parts, &mut shipped, "unpushed.bundle", 0, &mut || {
                    job.current(&format!("{}: bundling unpushed history", it.name));
                    let prod = producer("git", &bargs, &path, None)?;
                    match ship_stream(&p.server, &dest, "unpushed.bundle", prod, job, false) {
                        Ok((n, sha)) => {
                            let heads = remote(&p.server, &format!("git bundle list-heads {} | wc -l", q(&here("unpushed.bundle"))))?;
                            if heads.trim().parse::<u64>().unwrap_or(0) == 0 {
                                bail!("{}: the history bundle arrived empty", it.name);
                            }
                            Ok(Some((n, sha)))
                        }
                        // git refuses an empty bundle: every commit is already on a remote.
                        Err(e) if e.to_string().contains("empty bundle") => {
                            let _ = remote(&p.server, &format!("rm -f {}", q(&format!("{}.part", here("unpushed.bundle")))));
                            Ok(None)
                        }
                        Err(e) => Err(e),
                    }
                })?;
            }

            if wanted.iter().any(|(f, _)| f == "uncommitted.patch") {
                part(&mut parts, &mut shipped, "uncommitted.patch", 0, &mut || {
                    job.current(&format!("{}: saving uncommitted changes", it.name));
                    let prod = producer("git", &patch_args(it), &path, None)?;
                    ship_stream(&p.server, &dest, "uncommitted.patch", prod, job, false).map(Some)
                })?;
            }

            if !list.is_empty() {
                part(&mut parts, &mut shipped, "untracked.tar.gz", list_files, &mut || {
                    job.current(&format!("{}: packing untracked and ignored-but-unique files", it.name));
                    let mut data = Vec::new();
                    for f in &list {
                        data.extend_from_slice(f.as_bytes());
                        data.push(0);
                    }
                    let prod = producer(&tar_bin(), &untracked_args, &path, Some(data))?;
                    let (n, sha) = ship_stream(&p.server, &dest, "untracked.tar.gz", prod, job, true)?;
                    let there = remote_tar_count(&p.server, &here("untracked.tar.gz"))?;
                    if there != list_files {
                        bail!("{}: {} untracked files packed, {} arrived", it.name, list_files, there);
                    }
                    Ok(Some((n, sha)))
                })?;
            }

            let url = git(&path, &["remote", "get-url", "origin"]).unwrap_or_default().trim().to_string();
            let branch = meta.get("headRef").and_then(|x| x.as_str()).unwrap_or_default().to_string();
            restore.push_str(&format!(
                "Restore on the server:\n\n    git clone {url} {name} && cd {name}\n    git fetch {bundle} 'refs/*:refs/restored/*'   # if unpushed.bundle exists\n    git checkout {head}   # branch {branch}\n    git apply --binary {patch}             # if it exists\n    tar -xzf {untracked}                      # if it exists\n\nAll remotes, branches and upstreams are in {dest}/git-state.json.\n",
                url = if url.is_empty() { "<remote>".into() } else { url },
                name = it.name,
                head = meta.get("head").and_then(|x| x.as_str()).unwrap_or_default(),
                bundle = at(&parts, "unpushed.bundle", &dest),
                patch = at(&parts, "uncommitted.patch", &dest),
                untracked = at(&parts, "untracked.tar.gz", &dest),
            ));
        }
        if !dangling.is_empty() {
            restore.push_str("\nLinks whose target was already gone were not shipped (path -> target):\n\n");
            for d in &dangling {
                restore.push_str(&format!("    {d}\n"));
            }
        }
        if parts.iter().any(|x| !x.at.is_empty() && x.at != here(&x.file)) {
            restore.push_str("\nParts that had not changed since an earlier shipment were not sent again: the paths above point at them, and PARTS.json lists where every part is.\n");
        }
        ship_bytes(&p.server, &dest, "PARTS.json", serde_json::to_vec_pretty(&parts)?, job)?;
        ship_bytes(&p.server, &dest, "RESTORE.md", restore.clone().into_bytes(), job)?;
        Ok(restore)
    })();
    let restore = match sent {
        Ok(r) => r,
        Err(e) => {
            let cleaned = remote(&p.server, &format!("rm -rf -- {}", q(&dest))).is_ok();
            bail!("{e:#}{}", if cleaned { " (the partial shipment was removed from the server)" } else { "" });
        }
    };
    sent_record(&Sent { path: it.path.clone(), server: p.server.clone(), dest: dest.clone(), when: now_secs(), parts: parts.clone(), restore: restore.clone() });
    let artifacts = parts.iter().filter(|x| !x.at.is_empty()).map(|x| artifact_line(x, &dest)).collect();
    Ok(Shipped { dest, shipped, artifacts, restore, unchanged: false })
}

/// Where `file` of this shipment is on the server: its own folder, or the
/// earlier shipment an unchanged part was left in.
fn at(parts: &[Part], file: &str, dest: &str) -> String {
    parts.iter().find(|x| x.file == file && !x.at.is_empty()).map(|x| x.at.clone()).unwrap_or_else(|| format!("{dest}/{file}"))
}

fn patch_args(it: &Item) -> Vec<String> {
    vec!["-C".into(), it.path.clone(), "diff".into(), "HEAD".into(), "--binary".into()]
}

/// Write a small in-memory file to `<dest>/<file>` on the server and verify it.
fn ship_bytes(server: &str, dest: &str, file: &str, data: Vec<u8>, job: &Job) -> anyhow::Result<(u64, String)> {
    let mut sink = cmd(&ssh_bin())
        .args(ssh_args(server))
        .arg(format!("cat > {}", q(&format!("{dest}/{file}"))))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    {
        let mut inp = sink.stdin.take().ok_or_else(|| anyhow!("no ssh input"))?;
        inp.write_all(&data)?;
    }
    let st = sink.wait_with_output()?;
    if !st.status.success() {
        bail!("{file}: {}", String::from_utf8_lossy(&st.stderr).trim());
    }
    let sha = format!("{:x}", Sha256::digest(&data));
    let there = remote(server, &format!("sha256sum {} | cut -d' ' -f1", q(&format!("{dest}/{file}"))))?;
    if there.trim() != sha {
        bail!("{file}: checksum on the server does not match");
    }
    job.progress(data.len() as u64, 0, "");
    Ok((data.len() as u64, sha))
}

// ------------------------------------------------------------------ delete

/// Delete a folder tree, clearing read-only flags (git objects) first.
fn remove_tree(p: &Path) -> anyhow::Result<()> {
    for e in WalkDir::new(p).follow_links(false).into_iter().flatten() {
        if e.file_type().is_file() {
            if let Ok(m) = e.metadata() {
                let mut perm = m.permissions();
                if perm.readonly() {
                    #[allow(clippy::permissions_set_readonly_false)]
                    perm.set_readonly(false);
                    let _ = fs::set_permissions(e.path(), perm);
                }
            }
        }
    }
    fs::remove_dir_all(p).with_context(|| format!("delete {}", p.display()))
}

fn drop_local(it: &Item) -> anyhow::Result<()> {
    let path = PathBuf::from(&it.path);
    if it.kind == "worktree" && !it.parent.is_empty() {
        let _ = cmd("git").args(["-C", &it.parent, "worktree", "remove", "--force", "--force", &it.path]).output();
    }
    if path.exists() {
        remove_tree(&path)?;
    }
    if it.kind == "worktree" && !it.parent.is_empty() {
        let _ = cmd("git").args(["-C", &it.parent, "worktree", "prune"]).output();
    }
    Ok(())
}

// ------------------------------------------------------------------ run

/// Ship then delete each folder. Worktrees go before the repos they belong to.
pub fn run_items(p: &Policy, mut items: Vec<Item>, job: &Job) -> anyhow::Result<String> {
    if p.server.is_empty() {
        bail!("Set your server first.");
    }
    // Worktrees before their repos, then the cheapest shipment first, so most
    // of the space comes back early (a 27 GB checkout that ships 5 MB beats a
    // 10 GB backup folder that ships all of itself).
    items.sort_by_key(|i| (i.kind != "worktree", i.ship_bytes));
    let moving: HashSet<String> = items.iter().map(|i| norm_path(&i.path)).collect();
    job.set_total(items.iter().map(|i| i.ship_bytes).sum(), items.len() as u64);
    let (mut ok, mut freed, mut failed) = (0u64, 0u64, Vec::new());
    for it in items {
        if job.cancelled() {
            break;
        }
        // Re-measure: the folder may have changed since the scan.
        let fresh = inspect(p, Path::new(&it.path));
        let blockers: Vec<&String> = fresh.worktrees.iter().filter(|w| !moving.contains(&norm_path(w)) && Path::new(w).exists()).collect();
        if !blockers.is_empty() {
            let m = format!("{}: kept - worktree {} still uses it", fresh.name, blockers[0]);
            job.line(m.clone());
            failed.push(m);
            job.progress(0, 1, "");
            continue;
        }
        // A shipment whose local delete fails stays recorded as sent, so the
        // next run finds it unchanged and only retries the delete.
        match ship_item(p, &fresh, job).and_then(|s| match drop_local(&fresh) {
            Ok(()) => Ok(s),
            Err(e) => Err(anyhow!("shipped to {} but deleting it here failed ({e:#}); the next run only retries the delete", s.dest)),
        }) {
            Ok(s) => {
                ok += 1;
                freed += fresh.bytes;
                if s.unchanged {
                    job.line(format!("{}: moved ({} here, nothing shipped - unchanged since {})", fresh.name, human(fresh.bytes), s.dest));
                } else {
                    job.line(format!("{}: moved ({} here, {} shipped) -> {}", fresh.name, human(fresh.bytes), human(s.shipped), s.dest));
                }
                ledger_add(&Moved {
                    name: fresh.name.clone(),
                    path: fresh.path.clone(),
                    kind: fresh.kind.clone(),
                    server: p.server.clone(),
                    dest: s.dest,
                    when: now_secs(),
                    bytes: fresh.bytes,
                    shipped: s.shipped,
                    artifacts: s.artifacts,
                    restore: s.restore,
                });
            }
            Err(e) => {
                let m = format!("{}: kept - {e:#}", fresh.name);
                job.line(m.clone());
                failed.push(m);
            }
        }
        job.progress(0, 1, "");
    }
    Ok(format!("Moved {ok} folder(s), freed {} on this drive{}", human(freed), if failed.is_empty() { String::new() } else { format!("; {} kept (see details)", failed.len()) }))
}

fn norm_path(s: &str) -> String {
    s.trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase()
}

pub fn run_job(paths: Vec<String>) -> u64 {
    let p = policy();
    jobs::start("move", "Moving folders to the server", "offload", move |job| {
        let items: Vec<Item> = paths.iter().map(|x| inspect(&p, Path::new(x))).collect();
        run_items(&p, items, job)
    })
}

// ------------------------------------------------------------------ guard

/// One guard pass (the scheduled task): move idle folders, clean safe caches.
pub fn guard_run() -> String {
    let p = policy();
    let mut log = format!("[{}] guard: ", ymd_hms(now_secs()));
    if p.server.is_empty() || (p.roots.is_empty() && p.items.is_empty()) {
        log.push_str("not configured");
        append_log(&log);
        return log;
    }
    let id = jobs::start("move", "Move Weight guard", "offload", move |job| {
        let mut msgs = Vec::new();
        // A repo moves once its worktrees are gone, so run until a pass moves nothing.
        for _ in 0..3 {
            let scan = scan_now(&p, None);
            let due: Vec<Item> = scan
                .items
                .into_iter()
                .filter(|i| i.verdict == "safe" && i.idle_days >= p.idle_days as u64)
                .collect();
            if due.is_empty() || job.cancelled() {
                break;
            }
            let m = run_items(&p, due, job)?;
            let none = m.starts_with("Moved 0 ");
            msgs.push(m);
            if none {
                break;
            }
        }
        let mut msg = if msgs.is_empty() { "nothing idle enough to move".to_string() } else { msgs.join("; ") };
        if p.clean_caches {
            let ids: Vec<String> = ["win.temp", "win.dumps", "win.wer", "dev.npm", "dev.pip", "dev.yarn", "dev.go", "dev.crash"].iter().map(|s| s.to_string()).collect();
            let r = serde_json::to_value(crate::cleaner::clean(&ids)).unwrap_or(Value::Null);
            let b = r.get("bytes").and_then(|x| x.as_u64()).unwrap_or(0);
            msg.push_str(&format!("; caches cleaned: {}", human(b)));
        }
        Ok(msg)
    });
    // Headless: wait for the job and record what it did.
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some(j) = jobs::list().into_iter().find(|j| j.id == id) {
            if j.state != "running" {
                log.push_str(&j.message);
                for l in &j.output {
                    log.push_str(&format!("\n    {l}"));
                }
                break;
            }
        } else {
            break;
        }
    }
    append_log(&log);
    log
}

fn append_log(s: &str) {
    let path = data_dir().join("offload-guard.log");
    let mut all = fs::read_to_string(&path).unwrap_or_default();
    all.push_str(s);
    all.push('\n');
    let keep = tail(&all, 2000);
    let _ = fs::write(path, keep + "\n");
}

/// The guard runs MWM's own copy in the data folder, so it never waits for the
/// app to close for an update and never opens a window.
fn guard_exe() -> anyhow::Result<PathBuf> {
    let me = std::env::current_exe()?;
    let dir = data_dir().join("guard");
    fs::create_dir_all(&dir)?;
    let name = me.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "mwm-guard.exe".into());
    let target = dir.join(name);
    if me != target {
        let fresh = match (fs::metadata(&me), fs::metadata(&target)) {
            (Ok(a), Ok(b)) => a.len() == b.len() && a.modified().ok() <= b.modified().ok(),
            _ => false,
        };
        if !fresh {
            // Busy while a guard run is in progress: keep the current copy then.
            let _ = fs::copy(&me, &target);
        }
    }
    Ok(target)
}

/// Called when the app starts: a newer MWM refreshes the guard copy and task.
pub fn ensure_guard() {
    if cfg!(windows) && policy().guard {
        let _ = set_guard_task(true);
    }
}

fn guard_installed() -> bool {
    if !cfg!(windows) {
        return false;
    }
    cmd("schtasks").args(["/Query", "/TN", GUARD_TASK]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Register (or remove) the daily guard task for the signed-in user.
fn set_guard_task(on: bool) -> anyhow::Result<bool> {
    if !cfg!(windows) {
        return Ok(false);
    }
    if !on {
        let _ = cmd("schtasks").args(["/Delete", "/TN", GUARD_TASK, "/F"]).output();
        return Ok(false);
    }
    let exe = guard_exe()?.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$a = New-ScheduledTaskAction -Execute '{exe}' -Argument '--guard'; \
         $t1 = New-ScheduledTaskTrigger -Daily -At 4:15am; \
         $t2 = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME; $t2.Delay = 'PT20M'; \
         $s = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 12) -MultipleInstances IgnoreNew; \
         Register-ScheduledTask -TaskName '{GUARD_TASK}' -Action $a -Trigger $t1,$t2 -Settings $s -Description 'MWM - Move Weight Manager: moves idle folders to your server and cleans safe caches so this drive stops growing.' -Force | Out-Null; \
         @{{ ok = $true }} | ConvertTo-Json"
    );
    crate::util::powershell_json(&script)?;
    Ok(guard_installed())
}

// ------------------------------------------------------------------ misc

fn human(b: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b} B")
    } else {
        format!("{v:.1} {}", units[i])
    }
}

/// yyyymmdd (UTC) from Unix seconds.
fn ymd(secs: u64) -> String {
    let (y, m, d) = civil(secs / 86_400);
    format!("{y:04}{m:02}{d:02}")
}

/// hhmmss (UTC) from Unix seconds.
fn hms(secs: u64) -> String {
    let s = secs % 86_400;
    format!("{:02}{:02}{:02}", s / 3600, (s / 60) % 60, s % 60)
}

fn ymd_hms(secs: u64) -> String {
    let (y, m, d) = civil(secs / 86_400);
    let s = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z", s / 3600, (s / 60) % 60, s % 60)
}

fn civil(days: u64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(ymd(0), "19700101");
        assert_eq!(ymd(1_000_000_000), "20010909");
    }

    #[test]
    fn rebuildable_names() {
        assert!(is_rebuildable("node_modules/"));
        assert!(is_rebuildable("apps/web/dist/"));
        assert!(!is_rebuildable("public/cryptic-assets/"));
        assert!(!is_rebuildable(".env"));
    }

    #[test]
    fn quoting() {
        assert_eq!(q("a b"), "'a b'");
        assert_eq!(q("it's"), "'it'\\''s'");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mwm-offload-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn put(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn rebuildable_anywhere_in_the_path() {
        // tar's --exclude drops these too; counting them made the 2026-10 guard
        // fail every day ("52185 untracked files packed, 52175 arrived").
        assert!(is_rebuildable(".pytest_cache/README.md"));
        assert!(is_rebuildable("arcforge-studio/backend/.pytest_cache/v/cache/nodeids"));
        assert!(is_rebuildable("node_modules.partial/stream-buffers/coverage/lcov.info"));
        assert!(!is_rebuildable("node_modules.partial/stream-buffers/lib/index.js"));
        assert!(!is_rebuildable("tmp/qa-loop/distance.txt"));
    }

    #[test]
    fn manifest_counts_what_tar_keeps() {
        let d = scratch("manifest");
        put(&d, "a/keep.txt", "1");
        put(&d, "a/coverage/lcov.info", "2");
        put(&d, "a/deep/node_modules/x.js", "3");
        put(&d, "a/dist", "a file named like a build folder");
        put(&d, ".pytest_cache/README.md", "4");
        put(&d, "b.txt", "5");
        let entries = vec!["a".to_string(), "b.txt".to_string(), ".pytest_cache/README.md".to_string()];
        assert_eq!(manifest(&d, &entries, true).0, 2);
        assert_eq!(manifest(&d, &entries, false).0, 6);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn manifest_fingerprint_follows_content() {
        let d = scratch("fp");
        put(&d, "a/one.txt", "1");
        put(&d, "a/two.txt", "2");
        let entries = vec!["a".to_string()];
        let first = manifest(&d, &entries, true).1;
        assert_eq!(first, manifest(&d, &entries, true).1, "same files, same fingerprint");
        put(&d, "a/two.txt", "two");
        assert_ne!(first, manifest(&d, &entries, true).1, "a changed file changes it");
        let second = manifest(&d, &entries, true).1;
        put(&d, "a/three.txt", "3");
        assert_ne!(second, manifest(&d, &entries, true).1, "a new file changes it");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_links_are_split_off() {
        let d = scratch("dangling");
        put(&d, "real/file.txt", "x");
        std::os::unix::fs::symlink(d.join("real"), d.join("good")).unwrap();
        std::os::unix::fs::symlink("/nonexistent/mwm-spill", d.join("gone")).unwrap();
        let (keep, gone) = split_dangling(&d, vec!["real".into(), "good".into(), "gone".into()]);
        assert_eq!(keep, vec!["real".to_string(), "good".to_string()]);
        assert_eq!(gone, vec!["gone -> /nonexistent/mwm-spill".to_string()]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pruned_worktree_is_not_a_worktree() {
        let d = scratch("wt");
        fs::create_dir_all(d.join("repo/.git/worktrees/live")).unwrap();
        fs::create_dir_all(d.join("live")).unwrap();
        fs::create_dir_all(d.join("pruned")).unwrap();
        fs::write(d.join("live/.git"), format!("gitdir: {}\n", d.join("repo/.git/worktrees/live").display())).unwrap();
        fs::write(d.join("pruned/.git"), format!("gitdir: {}\n", d.join("repo/.git/worktrees/pruned").display())).unwrap();
        assert_eq!(worktree_parent(&d.join("live")), Some(d.join("repo")));
        assert_eq!(worktree_parent(&d.join("pruned")), None);
        let _ = fs::remove_dir_all(&d);
    }

    fn part(file: &str, fp: &str, at: &str) -> Part {
        Part { file: file.into(), fp: fp.into(), at: at.into(), sha: "s".into(), bytes: 10, files: 0 }
    }

    fn sent(parts: Vec<Part>) -> Sent {
        Sent { path: r"C:\MoveWeight\ui".into(), server: "root@h".into(), dest: "/r/h/20261025-ui".into(), parts, ..Default::default() }
    }

    fn w(x: &[(&str, &str)]) -> Vec<(String, String)> {
        x.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn unchanged_folder_ships_nothing() {
        let prev = sent(vec![
            part("git-state.json", "g1", "/r/h/20261025-ui/git-state.json"),
            part("unpushed.bundle", "b1", "/r/h/20261025-ui/unpushed.bundle"),
            part("untracked.tar.gz", "u1", "/r/h/20261025-ui/untracked.tar.gz"),
        ]);
        assert!(unchanged(&prev, &w(&[("git-state.json", "g1"), ("unpushed.bundle", "b1"), ("untracked.tar.gz", "u1")])));
        // One part changed: not unchanged, but the others are reusable.
        let changed = w(&[("git-state.json", "g1"), ("unpushed.bundle", "b1"), ("untracked.tar.gz", "u2")]);
        assert!(!unchanged(&prev, &changed));
        assert!(reusable(Some(&prev), "unpushed.bundle", "b1").is_some());
        assert!(reusable(Some(&prev), "untracked.tar.gz", "u2").is_none());
        // A new part (fresh uncommitted changes) or a part gone: changed.
        assert!(!unchanged(&prev, &w(&[("git-state.json", "g1"), ("unpushed.bundle", "b1"), ("uncommitted.patch", "p1"), ("untracked.tar.gz", "u1")])));
        assert!(!unchanged(&prev, &w(&[("git-state.json", "g1"), ("untracked.tar.gz", "u1")])));
        assert!(reusable(None, "git-state.json", "g1").is_none());
    }

    #[test]
    fn empty_bundle_counts_as_known() {
        // git refused an empty bundle last time: same refs, still nothing to ship.
        let prev = sent(vec![part("git-state.json", "g1", "/r/h/20261025-ui/git-state.json"), part("unpushed.bundle", "b1", "")]);
        assert!(unchanged(&prev, &w(&[("git-state.json", "g1"), ("unpushed.bundle", "b1")])));
    }

    #[test]
    fn sizes_from_stat() {
        let out = "3358119452 /r/h/20261025-ui/unpushed.bundle\n12 /r/h/with space/git-state.json\nstat: junk\n";
        assert_eq!(parse_sizes(out), vec![(3358119452, "/r/h/20261025-ui/unpushed.bundle"), (12, "/r/h/with space/git-state.json")]);
    }

    #[test]
    fn artifact_lines_say_where_parts_are() {
        let here = part("unpushed.bundle", "b", "/r/h/20261026-ui/unpushed.bundle");
        let kept = Part { files: 7, ..part("untracked.tar.gz", "u", "/r/h/20261025-ui/untracked.tar.gz") };
        assert_eq!(artifact_line(&here, "/r/h/20261026-ui"), "unpushed.bundle s");
        assert_eq!(artifact_line(&kept, "/r/h/20261026-ui"), "untracked.tar.gz s (7 files) - unchanged, kept at /r/h/20261025-ui/untracked.tar.gz");
        assert_eq!(at(&[kept.clone()], "untracked.tar.gz", "/r/h/20261026-ui"), "/r/h/20261025-ui/untracked.tar.gz");
        assert_eq!(at(&[kept], "uncommitted.patch", "/r/h/20261026-ui"), "/r/h/20261026-ui/uncommitted.patch");
    }

    /// Ships a scratch repo to a real server over SSH: first in full, then
    /// unchanged (nothing travels), then with one part changed (only that part
    /// travels, the rest is pointed at), then after a part vanished from the
    /// server (it travels again). The server must be this machine, since the
    /// test reads the shipment back from disk:
    /// `MWM_TEST_SERVER=root@127.0.0.1 cargo test -p mwm-core offload -- --ignored`
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn ships_once_then_points_at_what_is_there() {
        let Ok(server) = std::env::var("MWM_TEST_SERVER") else { return };
        let d = scratch("e2e");
        std::env::set_var("XDG_DATA_HOME", d.join("data"));
        let repo = d.join("work/proj");
        put(&repo, "tracked.txt", "v1");
        let g = |args: &[&str]| assert!(cmd("git").arg("-C").arg(&repo).args(args).status().unwrap().success());
        g(&["init", "-q"]);
        g(&["add", "."]);
        g(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"]);
        put(&repo, "tracked.txt", "v2");
        put(&repo, "notes/todo.md", "keep me");
        put(&repo, ".pytest_cache/README.md", "rebuildable");
        std::os::unix::fs::symlink("/nonexistent/mwm-spill", repo.join("spill")).unwrap();
        let p = Policy { server, remote_root: d.join("server").to_string_lossy().into(), ..Default::default() };
        let ship = || {
            let (tx, rx) = std::sync::mpsc::channel();
            let (p, it) = (p.clone(), inspect(&p, &repo));
            jobs::start("move", "test", "offload", move |job| {
                let _ = tx.send(ship_item(&p, &it, job).map(|s| (s.dest, s.unchanged, s.shipped)).map_err(|e| format!("{e:#}")));
                Ok(String::new())
            });
            rx.recv().unwrap().unwrap()
        };
        let files = |dest: &str| {
            let mut v: Vec<String> = fs::read_dir(dest).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
            v.sort();
            v
        };

        let (d1, same, n) = ship();
        assert!(!same && n > 0);
        assert_eq!(files(&d1), ["PARTS.json", "RESTORE.md", "git-state.json", "uncommitted.patch", "unpushed.bundle", "untracked.tar.gz"]);
        assert!(fs::read_to_string(format!("{d1}/RESTORE.md")).unwrap().contains("spill -> /nonexistent/mwm-spill"));

        let (d2, same, n) = ship();
        assert_eq!((d2.as_str(), same, n), (d1.as_str(), true, 0), "an unchanged folder ships nothing");

        put(&repo, "notes/todo.md", "keep me, changed");
        let (d3, same, _) = ship();
        assert!(!same && d3 != d1);
        assert_eq!(files(&d3), ["PARTS.json", "RESTORE.md", "git-state.json", "untracked.tar.gz"]);
        let restore = fs::read_to_string(format!("{d3}/RESTORE.md")).unwrap();
        assert!(restore.contains(&format!("{d1}/unpushed.bundle")) && restore.contains(&format!("{d1}/uncommitted.patch")));

        fs::remove_file(format!("{d1}/unpushed.bundle")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let (d4, same, _) = ship();
        assert!(!same && d4 != d3);
        assert_eq!(files(&d4), ["PARTS.json", "RESTORE.md", "git-state.json", "unpushed.bundle"]);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn tar_errors_name_the_real_cause() {
        let e = b"tar.exe: public/cr-realms: Cannot stat: No such file or directory\r\ntar.exe: Error exit delayed from previous errors\r\n";
        assert_eq!(tool_error(e, "x"), "tar.exe: public/cr-realms: Cannot stat: No such file or directory");
        assert_eq!(tool_error(b"tar.exe: Error exit delayed from previous errors\n", "local packing failed"), "local packing failed");
        assert_eq!(tool_error(b"a\nb\nc\nd\ne\n", "x"), "a; b; c (+2 more)");
        assert_eq!(hms(3_600 * 9 + 60 * 5 + 7), "090507");
    }
}
