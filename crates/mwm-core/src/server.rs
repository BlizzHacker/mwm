//! Server insight for Proxmox VE, Unraid and plain Linux (also shows Docker /
//! SMART on Windows when the tools exist): guests, storage, ZFS, SMART, Docker,
//! failed services, kernels - plus a few guarded one-click actions.

use serde_json::{json, Value};

use crate::util;

// Everything below collects from "the current node": this machine, or - while
// `info_on` runs for another Proxmox node - that node over the cluster's root SSH.
thread_local! {
    static ON: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn remote_ip() -> Option<String> {
    ON.with(|o| o.borrow().clone())
}

fn sh_quote(a: &str) -> String {
    format!("'{}'", a.replace('\'', r"'\''"))
}

fn ssh_args(ip: &str) -> Vec<String> {
    vec!["-o".into(), "BatchMode=yes".into(), "-o".into(), "ConnectTimeout=6".into(), format!("root@{ip}")]
}

fn ssh(ip: &str, cmd: &str) -> anyhow::Result<(i32, String)> {
    let mut a = ssh_args(ip);
    a.push(cmd.to_string());
    let r: Vec<&str> = a.iter().map(String::as_str).collect();
    util::run_capture("ssh", &r)
}

fn capture(prog: &str, args: &[&str]) -> anyhow::Result<(i32, String)> {
    match remote_ip() {
        None => util::run_capture(prog, args),
        Some(ip) => {
            let cmd = std::iter::once(prog.to_string()).chain(args.iter().map(|a| sh_quote(a))).collect::<Vec<_>>().join(" ");
            ssh(&ip, &cmd)
        }
    }
}

fn run(prog: &str, args: &[&str]) -> Option<String> {
    capture(prog, args).ok().filter(|(c, _)| *c == 0).map(|(_, o)| o)
}

fn exists(p: &str) -> bool {
    match remote_ip() {
        None => std::path::Path::new(p).exists(),
        Some(_) => capture("test", &["-e", p]).map(|(c, _)| c == 0).unwrap_or(false),
    }
}

fn read(p: &str) -> Option<String> {
    match remote_ip() {
        None => std::fs::read_to_string(p).ok(),
        Some(_) => run("cat", &[p]),
    }
}

/// Filesystems mounted on the current node (USB disks, NFS/CIFS shares, ...).
fn mounts() -> Value {
    let out = run("df", &["-PT", "-B1", "-x", "tmpfs", "-x", "devtmpfs", "-x", "overlay", "-x", "squashfs", "-x", "autofs", "-x", "efivarfs"]).unwrap_or_default();
    let v: Vec<Value> = out
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 7 {
                return None;
            }
            let mount = f[6..].join(" ");
            // Container rootfs mounts and runtime dirs are noise here.
            if mount.starts_with("/var/lib/lxc/") || mount.starts_with("/run") || mount.starts_with("/proc") {
                return None;
            }
            Some(json!({"source": f[0], "fs": f[1], "total": f[2].parse::<u64>().unwrap_or(0), "used": f[3].parse::<u64>().unwrap_or(0), "free": f[4].parse::<u64>().unwrap_or(0), "mount": mount}))
        })
        .collect();
    json!(v)
}

/// Proxmox cluster overview: every node with CPU / RAM / uptime, and IPs.
fn cluster_nodes() -> Value {
    let members: Value = std::fs::read_to_string("/etc/pve/.members").ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
    let nodes: Vec<Value> = util::run_capture("pvesh", &["get", "/cluster/resources", "--type", "node", "--output-format", "json"])
        .ok()
        .and_then(|(_, o)| serde_json::from_str::<Vec<Value>>(&o).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|n| {
            let name = n["node"].as_str().unwrap_or("").to_string();
            json!({"node": name, "status": n["status"], "cpu": n["cpu"], "maxcpu": n["maxcpu"], "mem": n["mem"], "maxmem": n["maxmem"], "disk": n["disk"], "maxdisk": n["maxdisk"], "uptime": n["uptime"], "ip": members["nodelist"][&name]["ip"], "local": name.eq_ignore_ascii_case(&local_node())})
        })
        .collect();
    json!(nodes)
}

/// `info()` for any node of the local Proxmox cluster ("" = this machine).
pub fn info_on(node: &str) -> anyhow::Result<Value> {
    if node.is_empty() || node.eq_ignore_ascii_case(&local_node()) {
        return Ok(info());
    }
    safe_token(node)?;
    let ip = node_ip(node).ok_or_else(|| anyhow::anyhow!("{node} is not a node of this cluster"))?;
    ON.with(|o| *o.borrow_mut() = Some(ip));
    let mut v = info();
    ON.with(|o| *o.borrow_mut() = None);
    // Cluster-wide facts are the same from every node; keep this node's view.
    v["node"] = json!(node);
    Ok(v)
}

/// Install MWM's web server on a cluster node and return its sign-in link.
pub fn deploy_node(node: &str) -> anyhow::Result<Value> {
    safe_token(node)?;
    let local = node.is_empty() || node.eq_ignore_ascii_case(&local_node());
    let exe = std::env::current_exe()?;
    let unit = "[Unit]\\nDescription=MWM - Move Weight Manager web UI\\nAfter=network-online.target\\n[Service]\\nEnvironment=XDG_DATA_HOME=/var/lib\\nExecStart=/usr/local/bin/mwm serve --bind 0.0.0.0:7777\\nRestart=on-failure\\n[Install]\\nWantedBy=multi-user.target\\n";
    let setup = format!("printf '{unit}' > /etc/systemd/system/mwm-web.service && systemctl daemon-reload && systemctl enable mwm-web >/dev/null 2>&1 && systemctl restart mwm-web && sleep 1 && XDG_DATA_HOME=/var/lib /usr/local/bin/mwm serve --show-token && hostname -I");
    let (code, out) = if local {
        std::fs::copy(&exe, "/usr/local/bin/mwm.new")?;
        std::fs::rename("/usr/local/bin/mwm.new", "/usr/local/bin/mwm")?;
        util::run_capture("sh", &["-c", &format!("chmod 755 /usr/local/bin/mwm && {setup}")])?
    } else {
        let ip = node_ip(node).ok_or_else(|| anyhow::anyhow!("{node} is not a node of this cluster"))?;
        let mut a: Vec<String> = ssh_args(&ip)[..4].to_vec();
        a.push(exe.to_string_lossy().into());
        a.push(format!("root@{ip}:/usr/local/bin/mwm.new"));
        let r: Vec<&str> = a.iter().map(String::as_str).collect();
        let (c, o) = util::run_capture("scp", &r)?;
        if c != 0 {
            anyhow::bail!("copying MWM to {node} failed: {}", o.trim());
        }
        ssh(&ip, &format!("mv /usr/local/bin/mwm.new /usr/local/bin/mwm && chmod 755 /usr/local/bin/mwm && {setup}"))?
    };
    if code != 0 {
        anyhow::bail!("setting up MWM on {node} failed: {}", out.trim());
    }
    let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
    let token = lines.next().unwrap_or("").to_string();
    let ip = lines.next().and_then(|l| l.split_whitespace().next()).unwrap_or("").to_string();
    Ok(json!({"node": node, "link": format!("http://{ip}:7777/?token={token}"), "url": format!("http://{ip}:7777")}))
}

pub fn platform() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if exists("/usr/bin/pveversion") {
        "proxmox"
    } else if exists("/etc/unraid-version") {
        "unraid"
    } else if read("/etc/version").map(|s| s.contains("TrueNAS")).unwrap_or(false) {
        "truenas"
    } else {
        "linux"
    }
}

fn ini(path: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let text = read(path).unwrap_or_default();
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') && l.ends_with(']') {
            out.push((l.trim_matches(['[', ']', '"']).to_string(), Vec::new()));
        } else if let Some((k, v)) = l.split_once('=') {
            let kv = (k.trim().to_string(), v.trim().trim_matches('"').to_string());
            match out.last_mut() {
                Some(s) => s.1.push(kv),
                None => out.push((String::new(), vec![kv])),
            }
        }
    }
    out
}

fn proxmox() -> Value {
    if !exists("/usr/bin/pveversion") {
        return Value::Null;
    }
    let version = run("pveversion", &[]).unwrap_or_default().trim().to_string();
    let guests: Value = run("pvesh", &["get", "/cluster/resources", "--type", "vm", "--output-format", "json"])
        .and_then(|o| serde_json::from_str(&o).ok())
        .unwrap_or(json!([]));
    let storage: Vec<Value> = run("pvesm", &["status"])
        .map(|o| {
            o.lines()
                .skip(1)
                .filter_map(|l| {
                    let f: Vec<&str> = l.split_whitespace().collect();
                    // Skip header noise and warnings like "unable to activate storage ...".
                    (f.len() >= 6 && matches!(f[2], "active" | "inactive" | "disabled")).then(|| json!({"name": f[0], "type": f[1], "status": f[2], "total": f[3].parse::<u64>().unwrap_or(0) * 1024, "used": f[4].parse::<u64>().unwrap_or(0) * 1024, "avail": f[5].parse::<u64>().unwrap_or(0) * 1024}))
                })
                .collect()
        })
        .unwrap_or_default();
    let subscription = run("pvesubscription", &["get"]).and_then(|o| o.lines().find_map(|l| l.trim().strip_prefix("status:").map(|s| s.trim().to_string()))).unwrap_or_default();
    json!({"version": version, "guests": guests, "storage": storage, "subscription": subscription})
}

fn unraid() -> Value {
    if !exists("/etc/unraid-version") {
        return Value::Null;
    }
    let version = read("/etc/unraid-version").unwrap_or_default().replace("version=", "").trim().trim_matches('"').to_string();
    let var = ini("/var/local/emhttp/var.ini");
    let get = |k: &str| var.iter().flat_map(|s| s.1.iter()).find(|(kk, _)| kk == k).map(|(_, v)| v.clone()).unwrap_or_default();
    let disks: Vec<Value> = ini("/var/local/emhttp/disks.ini")
        .into_iter()
        .map(|(name, kv)| {
            let g = |k: &str| kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone()).unwrap_or_default();
            json!({"name": name, "device": g("device"), "status": g("status"), "temp": g("temp"), "size": g("size").parse::<u64>().unwrap_or(0) * 1024, "free": g("fsFree").parse::<u64>().unwrap_or(0) * 1024, "fs": g("fsType"), "errors": g("numErrors")})
        })
        .filter(|d| d["device"].as_str().map(|s| !s.is_empty()).unwrap_or(false))
        .collect();
    json!({"version": version, "array": get("mdState"), "parity": get("sbSyncErrs"), "disks": disks})
}

fn zfs() -> Value {
    let Some(list) = run("zpool", &["list", "-H", "-p", "-o", "name,size,alloc,free,frag,cap,health"]) else { return json!([]) };
    let pools: Vec<Value> = list
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 7 {
                return None;
            }
            let status = run("zpool", &["status", f[0]]).unwrap_or_default();
            let scan = status.lines().find_map(|l| l.trim().strip_prefix("scan:").map(|s| s.trim().to_string())).unwrap_or_default();
            let errors = status.lines().find_map(|l| l.trim().strip_prefix("errors:").map(|s| s.trim().to_string())).unwrap_or_default();
            Some(json!({"name": f[0], "size": f[1].parse::<u64>().unwrap_or(0), "alloc": f[2].parse::<u64>().unwrap_or(0), "free": f[3].parse::<u64>().unwrap_or(0), "frag": f[4], "cap": f[5], "health": f[6], "scan": scan, "errors": errors}))
        })
        .collect();
    json!(pools)
}

fn docker() -> Value {
    let Some(ps) = run("docker", &["ps", "-a", "--format", "{{json .}}"]) else { return Value::Null };
    let containers: Vec<Value> = ps
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|c| json!({"name": c["Names"], "image": c["Image"], "state": c["State"], "status": c["Status"], "id": c["ID"]}))
        .collect();
    let df: Vec<Value> = run("docker", &["system", "df", "--format", "{{json .}}"])
        .map(|o| o.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).map(|d| json!({"type": d["Type"], "size": d["Size"], "reclaimable": d["Reclaimable"], "count": d["TotalCount"]})).collect())
        .unwrap_or_default();
    json!({"containers": containers, "df": df})
}

fn smart() -> Value {
    let Some(scan) = run("smartctl", &["--scan", "-j"]) else { return Value::Null };
    let devs: Vec<String> = serde_json::from_str::<Value>(&scan)
        .ok()
        .and_then(|v| v["devices"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|d| d["name"].as_str().map(String::from))
        .collect();
    let disks: Vec<Value> = devs
        .iter()
        .filter_map(|d| {
            // smartctl exits non-zero for warnings; parse regardless.
            let (_, out) = capture("smartctl", &["-j", "-H", "-A", "-i", d]).ok()?;
            let v: Value = serde_json::from_str(&out).ok()?;
            let attr = |id: u64| v["ata_smart_attributes"]["table"].as_array().and_then(|t| t.iter().find(|a| a["id"].as_u64() == Some(id))).and_then(|a| a["raw"]["value"].as_u64());
            Some(json!({
                "device": d,
                "model": v["model_name"].as_str().or(v["model_family"].as_str()).unwrap_or(""),
                "capacity": v["user_capacity"]["bytes"].as_u64().or(v["nvme_total_capacity"].as_u64()),
                "passed": v["smart_status"]["passed"].as_bool(),
                "temp": v["temperature"]["current"].as_u64(),
                "hours": v["power_on_time"]["hours"].as_u64(),
                "wear": v["nvme_smart_health_information_log"]["percentage_used"].as_u64(),
                "reallocated": attr(5),
                "pending": attr(197),
                "media_errors": v["nvme_smart_health_information_log"]["media_errors"].as_u64(),
                "rotation": v["rotation_rate"].as_u64(),
            }))
        })
        .collect();
    json!(disks)
}

pub fn info() -> Value {
    let loadavg = read("/proc/loadavg").map(|s| s.split_whitespace().take(3).collect::<Vec<_>>().join(" ")).unwrap_or_default();
    let failed: Vec<String> = run("systemctl", &["--failed", "--no-legend", "--plain"])
        .map(|o| o.lines().filter_map(|l| l.split_whitespace().next().map(String::from)).collect())
        .unwrap_or_default();
    let kernel = run("uname", &["-r"]).unwrap_or_default().trim().to_string();
    let kernels: Vec<String> = run("dpkg-query", &["-W", "-f=${Package} ${db:Status-Abbrev}\\n", "proxmox-kernel-*", "pve-kernel-*", "linux-image-[0-9]*"])
        .map(|o| o.lines().filter(|l| l.contains(" ii")).filter_map(|l| l.split_whitespace().next().map(String::from)).filter(|p| p.chars().any(|c| c.is_ascii_digit()) && !p.ends_with("-signed")).collect())
        .unwrap_or_default();
    json!({
        "platform": platform(),
        "node": read("/etc/hostname").map(|h| h.trim().to_string()).unwrap_or_default(),
        "nodes": if remote_ip().is_none() && exists("/etc/pve/.members") { cluster_nodes() } else { Value::Null },
        "mounts": mounts(),
        "kernel": kernel,
        "kernels": kernels,
        "load": loadavg,
        "failed": failed,
        "proxmox": proxmox(),
        "unraid": unraid(),
        "zfs": zfs(),
        "docker": docker(),
        "smart": smart(),
    })
}

// ------------------------------------------------ Docker inside LXC guests ----
//
// `pct exec` only reaches containers on the local node; guests elsewhere in
// the cluster go through the root SSH trust Proxmox sets up between nodes.
// Only fixed scripts are run; node / vmid / container names are validated.

fn local_node() -> String {
    std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()).unwrap_or_default()
}

fn node_ip(node: &str) -> Option<String> {
    let m: Value = serde_json::from_str(&std::fs::read_to_string("/etc/pve/.members").ok()?).ok()?;
    m["nodelist"][node]["ip"].as_str().map(String::from)
}

/// Run a fixed `sh -c` script inside LXC `vmid` on `node`.
pub(crate) fn in_guest(node: &str, vmid: &str, script: &str) -> anyhow::Result<(i32, String)> {
    safe_token(node)?;
    if vmid.is_empty() || !vmid.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("bad container id {vmid}");
    }
    if node.eq_ignore_ascii_case(&local_node()) {
        return util::run_capture("pct", &["exec", vmid, "--", "sh", "-c", script]);
    }
    let ip = node_ip(node).ok_or_else(|| anyhow::anyhow!("node {node} is not in this cluster"))?;
    // Script travels as one single-quoted argument of the remote pct command.
    let remote = format!("pct exec {vmid} -- sh -c '{}'", script.replace('\'', r"'\''"));
    util::run_capture("ssh", &["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", &format!("root@{ip}"), &remote])
}

/// Like `in_guest`, but streams `input` to the script's stdin (file uploads).
pub(crate) fn in_guest_input(node: &str, vmid: &str, script: &str, input: &[u8]) -> anyhow::Result<(i32, String)> {
    safe_token(node)?;
    if vmid.is_empty() || !vmid.chars().all(|c| c.is_ascii_digit()) {
        anyhow::bail!("bad container id {vmid}");
    }
    if node.eq_ignore_ascii_case(&local_node()) {
        return util::run_input("pct", &["exec", vmid, "--", "sh", "-c", script], input);
    }
    let ip = node_ip(node).ok_or_else(|| anyhow::anyhow!("node {node} is not in this cluster"))?;
    let remote = format!("pct exec {vmid} -- sh -c '{}'", script.replace('\'', r"'\''"));
    util::run_input("ssh", &["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", &format!("root@{ip}"), &remote], input)
}

const SEP: &str = "---MWM-SPLIT---";

/// Containers + disk usage of Docker running inside an LXC.
pub fn guest_docker(node: &str, vmid: &str) -> anyhow::Result<Value> {
    let script = format!(
        "command -v docker >/dev/null 2>&1 || exit 42; docker ps -a --format '{{{{json .}}}}'; echo {SEP}; docker system df --format '{{{{json .}}}}'; echo {SEP}; docker images -f dangling=true -q | wc -l"
    );
    let (code, out) = in_guest(node, vmid, &script)?;
    if code == 42 {
        return Ok(json!({"available": false}));
    }
    if code != 0 {
        anyhow::bail!("docker in CT {vmid} failed: {}", out.trim().lines().last().unwrap_or(""));
    }
    let parts: Vec<&str> = out.split(SEP).collect();
    let containers: Vec<Value> = parts
        .first()
        .unwrap_or(&"")
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .map(|c| json!({"name": c["Names"], "image": c["Image"], "state": c["State"], "status": c["Status"], "size": c["Size"], "id": c["ID"]}))
        .collect();
    let df: Vec<Value> = parts
        .get(1)
        .unwrap_or(&"")
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .map(|d| json!({"type": d["Type"], "count": d["TotalCount"], "size": d["Size"], "reclaimable": d["Reclaimable"]}))
        .collect();
    let dangling = parts.get(2).and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    Ok(json!({"available": true, "containers": containers, "df": df, "dangling_images": dangling}))
}

/// Container actions and cleanups inside an LXC. Volumes are never pruned.
pub fn guest_docker_action(node: &str, vmid: &str, action: &str, target: &str) -> anyhow::Result<String> {
    let cmd = match action {
        "start" | "stop" | "restart" => format!("docker {action} {}", safe_token(target)?),
        "remove" => format!("docker rm {}", safe_token(target)?), // refuses running containers
        "prune_containers" => "docker container prune -f".into(),
        "prune_images" => "docker image prune -f".into(),
        "prune_images_unused" => "docker image prune -a -f".into(),
        "prune_builder" => "docker builder prune -f".into(),
        "prune_networks" => "docker network prune -f".into(),
        "cleanup" => "docker container prune -f; docker image prune -f; docker builder prune -f; docker network prune -f".into(),
        other => anyhow::bail!("unknown docker action {other}"),
    };
    let (code, out) = in_guest(node, vmid, &format!("{cmd} 2>&1; echo; docker system df 2>/dev/null | tail -n +2"))?;
    if code != 0 {
        anyhow::bail!("{}", out.trim());
    }
    let reclaimed: Vec<&str> = out.lines().filter(|l| l.contains("Total reclaimed space")).collect();
    Ok(if reclaimed.is_empty() { out.trim().lines().take(6).collect::<Vec<_>>().join(" / ") } else { reclaimed.join(" / ") })
}

/// Which running LXCs in the cluster have Docker, as a background job.
pub fn docker_scan_cluster() -> u64 {
    crate::jobs::start("docker-scan", "Find Docker in every LXC", "server", |job| {
        let guests: Vec<Value> = run("pvesh", &["get", "/cluster/resources", "--type", "vm", "--output-format", "json"])
            .and_then(|o| serde_json::from_str::<Vec<Value>>(&o).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|g| g["type"] == "lxc" && g["status"] == "running")
            .collect();
        job.set_total(0, guests.len() as u64);
        let mut found = 0;
        for g in guests {
            if job.cancelled() {
                break;
            }
            let (node, vmid, name) = (g["node"].as_str().unwrap_or(""), g["vmid"].to_string(), g["name"].as_str().unwrap_or(""));
            job.progress(0, 1, &format!("CT {vmid} {name}"));
            let script = format!("command -v docker >/dev/null 2>&1 || exit 42; echo $(docker ps -q | wc -l) $(docker ps -aq | wc -l); echo {SEP}; docker system df --format '{{{{.Type}}}}|{{{{.Size}}}}|{{{{.Reclaimable}}}}'");
            if let Ok((0, out)) = in_guest(node, &vmid, &script) {
                let mut parts = out.split(SEP);
                let counts: Vec<&str> = parts.next().unwrap_or("").split_whitespace().collect();
                let df = parts.next().unwrap_or("").lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(";");
                found += 1;
                // One JSON line per Docker host - the UI reads these from the job output.
                job.line(json!({"node": node, "vmid": vmid, "name": name, "running": counts.first().copied().unwrap_or("0"), "total": counts.get(1).copied().unwrap_or("0"), "df": df}).to_string());
            }
        }
        Ok(format!("{found} LXC(s) run Docker"))
    })
}

fn safe_token(s: &str) -> anyhow::Result<&str> {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./@:".contains(c)) {
        anyhow::bail!("invalid target `{s}`");
    }
    Ok(s)
}

/// Guarded server actions. `target` is validated to a plain identifier.
pub fn action(action: &str, target: &str) -> anyhow::Result<String> {
    let t = safe_token(target)?;
    let (prog, args): (&str, Vec<String>) = match action {
        "guest_start" | "guest_shutdown" | "guest_reboot" => {
            // node/type/vmid through the cluster API, so guests on any node work.
            let parts: Vec<&str> = t.split('/').collect();
            let [node, kind, id] = parts.as_slice() else { anyhow::bail!("target must be <node>/<qemu|lxc>/<vmid>") };
            if !matches!(*kind, "qemu" | "lxc") || !id.chars().all(|c| c.is_ascii_digit()) {
                anyhow::bail!("bad guest target {t}");
            }
            let verb = action.trim_start_matches("guest_");
            ("pvesh", vec!["create".into(), format!("/nodes/{node}/{kind}/{id}/status/{verb}")])
        }
        "docker_start" | "docker_stop" | "docker_restart" => ("docker", vec![action.trim_start_matches("docker_").into(), t.into()]),
        "zfs_scrub" => ("zpool", vec!["scrub".into(), t.into()]),
        "smart_test" => ("smartctl", vec!["-t".into(), "short".into(), t.into()]),
        "service_restart" => ("systemctl", vec!["restart".into(), t.into()]),
        other => anyhow::bail!("unknown action {other}"),
    };
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (code, out) = util::run_capture(prog, &a)?;
    if code != 0 {
        anyhow::bail!("{prog} failed: {}", out.trim());
    }
    Ok(if out.trim().is_empty() { format!("{action} {t}: done") } else { out.trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_metacharacters() {
        assert!(safe_token("pve1/qemu/100").is_ok());
        assert!(safe_token("sda; rm -rf /").is_err());
        assert!(safe_token("$(reboot)").is_err());
        assert!(action("nope", "x").is_err());
    }

    #[test]
    fn parses_ini() {
        let p = std::env::temp_dir().join(format!("mwm-ini-{}", std::process::id()));
        std::fs::write(&p, "[\"disk1\"]\nname=\"disk1\"\ndevice=\"sdb\"\ntemp=\"34\"\n").unwrap();
        let v = ini(&p.to_string_lossy());
        assert_eq!(v[0].0, "disk1");
        assert_eq!(v[0].1[1], ("device".to_string(), "sdb".to_string()));
        std::fs::remove_file(p).unwrap();
    }
}
