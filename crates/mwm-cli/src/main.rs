//! `mwm` - Move Weight Manager from the command line (servers, SSH, scripts).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use mwm_core::{apps, cleaner, disk, drivers, dupes, jobs, keys, procs, server, shred, startup, sys, toolkit, updater};

mod serve;

#[derive(Parser)]
#[command(name = "mwm", version, about = "MWM - Move Weight Manager: clean junk, uninstall cleanly, tame startup.")]
struct Cli {
    /// Print machine-readable JSON instead of tables.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Machine summary: OS, disks, elevation.
    Info,
    /// Measure every cleaning rule (nothing is deleted).
    Scan {
        /// Only show rules that are on by default.
        #[arg(long)]
        defaults: bool,
    },
    /// Delete junk. Without --ids, cleans the default-on rules.
    Clean {
        /// Comma-separated rule ids (see `mwm scan`).
        #[arg(long, value_delimiter = ',')]
        ids: Vec<String>,
        /// Actually delete. Without this, prints what would be cleaned.
        #[arg(long)]
        yes: bool,
    },
    /// List installed programs.
    Apps {
        /// Filter by name (case-insensitive substring).
        filter: Option<String>,
    },
    /// Uninstall a program by id (see `mwm apps --json`), then list leftovers.
    Uninstall {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Show leftovers for an installed program id.
    Leftovers { id: String },
    /// Startup programs, scheduled tasks and services.
    Startup {
        /// Enable (`on`) or disable (`off`) an item id.
        #[arg(long, num_args = 2, value_names = ["ID", "on|off"])]
        set: Option<Vec<String>>,
    },
    /// Software updates from the OS package manager.
    Updates {
        /// Install updates for these ids (comma-separated) or `all`.
        #[arg(long, value_delimiter = ',')]
        apply: Vec<String>,
    },
    /// Find duplicate files.
    Dupes {
        roots: Vec<PathBuf>,
        #[arg(long, default_value_t = 1_048_576)]
        min_size: u64,
    },
    /// What is taking space under a folder.
    Analyze {
        root: PathBuf,
        #[arg(long, default_value_t = 25)]
        top: usize,
    },
    /// Overwrite then delete files/folders (unrecoverable).
    Shred {
        paths: Vec<String>,
        #[arg(long, default_value_t = 1)]
        passes: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Overwrite free space on the volume holding DIR.
    WipeFree { dir: String },
    /// Installed drivers (Windows) / kernel modules (Linux).
    Drivers,
    /// Serve the full MWM interface in a browser (Proxmox / Unraid / headless).
    Serve {
        /// Address to listen on. Use 0.0.0.0:7777 to reach it from your LAN.
        #[arg(long, default_value = "127.0.0.1:7777")]
        bind: String,
        /// Refuse every action that changes the machine.
        #[arg(long)]
        read_only: bool,
        /// Generate a new access token (signs everyone out).
        #[arg(long)]
        new_token: bool,
        /// Print the access token and exit.
        #[arg(long)]
        show_token: bool,
    },
    /// Product keys, licenses, BitLocker recovery keys, Wi-Fi / SSH keys.
    Keys,
    /// Hardware / OS / license report.
    Report,
    /// Proxmox guests, ZFS, SMART, Docker, failed services.
    Server,
    /// Run a repair task (see `mwm repair --list`).
    Repair {
        task: Option<String>,
        #[arg(long)]
        list: bool,
    },
    /// Running programs by memory use.
    Top {
        #[arg(long, default_value_t = 25)]
        n: usize,
    },
}

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

fn emit<T: serde::Serialize>(json: bool, v: &T, text: impl FnOnce()) {
    if json {
        println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
    } else {
        text();
    }
}

fn main() -> anyhow::Result<()> {
    // Behave like a normal Unix tool when piped into `head`.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let j = cli.json;
    match cli.cmd {
        Cmd::Info => {
            let i = sys::info();
            emit(j, &i, || {
                println!("MWM {} on {} ({}){}", i.version, i.hostname, i.os, if i.elevated { " [admin]" } else { "" });
                println!("CPU {} x{}  RAM {} / {}", i.cpu, i.cores, human(i.memory_used), human(i.memory_total));
                for d in &i.disks {
                    println!("  {:<22} {:>10} free of {:>10}  {}", d.mount, human(d.free), human(d.total), d.fs);
                }
            });
        }
        Cmd::Scan { defaults } => {
            let mut items = cleaner::scan_all();
            if defaults {
                items.retain(|i| i.default_on);
            }
            emit(j, &items, || {
                let mut total = 0;
                for i in &items {
                    if i.bytes == 0 {
                        continue;
                    }
                    total += i.bytes;
                    println!(
                        "{} {:<24} {:<18} {:<34} {:>10} {:>8} files{}",
                        if i.default_on { "*" } else { " " },
                        i.id,
                        i.category,
                        i.name,
                        human(i.bytes),
                        i.files,
                        if i.admin { "  (admin)" } else { "" }
                    );
                }
                println!("\n{} found across all rules (* = cleaned by default).", human(total));
            });
        }
        Cmd::Clean { ids, yes } => {
            let ids = if ids.is_empty() { cleaner::default_ids() } else { ids };
            if !yes {
                let items: Vec<_> = cleaner::scan_all().into_iter().filter(|i| ids.contains(&i.id)).collect();
                let total: u64 = items.iter().map(|i| i.bytes).sum();
                emit(j, &items, || {
                    for i in &items {
                        println!("  {:<24} {:>10}", i.id, human(i.bytes));
                    }
                    println!("Would free {}. Re-run with --yes to clean.", human(total));
                });
                return Ok(());
            }
            let res = cleaner::clean(&ids);
            emit(j, &res, || {
                let total: u64 = res.iter().map(|r| r.bytes_freed).sum();
                for r in &res {
                    println!("  {:<24} freed {:>10}  ({} files, {} skipped)", r.id, human(r.bytes_freed), r.files_deleted, r.skipped);
                }
                println!("Moved {} of junk.", human(total));
            });
        }
        Cmd::Apps { filter } => {
            let mut list = apps::list(true);
            if let Some(f) = filter {
                let f = f.to_lowercase();
                list.retain(|a| a.name.to_lowercase().contains(&f));
            }
            emit(j, &list, || {
                for a in &list {
                    println!("{:<48} {:<18} {:<28} {:>9}", a.name, a.version, a.publisher.chars().take(28).collect::<String>(), if a.size_bytes > 0 { human(a.size_bytes) } else { String::new() });
                }
                println!("{} programs", list.len());
            });
        }
        Cmd::Uninstall { id, yes } => {
            let all = apps::list(true);
            let Some(app) = all.iter().find(|a| a.id == id) else { anyhow::bail!("no installed program with id {id}") };
            if !yes {
                println!("Would uninstall {} {} ({}). Re-run with --yes.", app.name, app.version, app.source);
                return Ok(());
            }
            let out = apps::uninstall(app, false);
            let left = apps::leftovers(app);
            emit(j, &(&out, &left), || {
                println!("{}", out.message);
                for l in &left {
                    println!("  leftover {:<6} {} {}", l.kind, l.path, if l.bytes > 0 { human(l.bytes) } else { String::new() });
                }
            });
        }
        Cmd::Leftovers { id } => {
            let all = apps::list(true);
            let Some(app) = all.iter().find(|a| a.id == id) else { anyhow::bail!("no installed program with id {id}") };
            let left = apps::leftovers(app);
            emit(j, &left, || {
                for l in &left {
                    println!("{:<6} {} {}", l.kind, l.path, human(l.bytes));
                }
            });
        }
        Cmd::Startup { set } => {
            if let Some(v) = set {
                let r = startup::set_enabled(&v[0], matches!(v[1].as_str(), "on" | "enable" | "enabled" | "1" | "true"));
                println!("{}", r.message);
                return Ok(());
            }
            let items = startup::list();
            emit(j, &items, || {
                for i in &items {
                    println!("[{}] {:<9} {:<40} {}", if i.enabled { "on " } else { "off" }, i.kind, i.name, i.id);
                }
            });
        }
        Cmd::Updates { apply } => {
            if !apply.is_empty() {
                let ids: Vec<String> = if apply.iter().any(|a| a == "all") { updater::list()?.into_iter().map(|u| u.id).collect() } else { apply };
                let res: Vec<_> = ids.iter().map(|i| updater::apply(i)).collect();
                emit(j, &res, || {
                    for r in &res {
                        println!("{} {}: {}", if r.ok { "OK  " } else { "FAIL" }, r.id, r.message);
                    }
                });
                return Ok(());
            }
            let list = updater::list()?;
            emit(j, &list, || {
                for u in &list {
                    println!("{:<44} {:<16} -> {:<16} {}", u.name, u.current, u.available, u.id);
                }
                println!("{} updates available", list.len());
            });
        }
        Cmd::Dupes { roots, min_size } => {
            let roots = if roots.is_empty() { vec![PathBuf::from(".")] } else { roots };
            let groups = dupes::find(&roots, min_size, false);
            emit(j, &groups, || {
                let wasted: u64 = groups.iter().map(|g| g.wasted).sum();
                for g in groups.iter().take(50) {
                    println!("{} x{} ({} wasted)", human(g.size), g.files.len(), human(g.wasted));
                    for f in &g.files {
                        println!("    {}", f.path);
                    }
                }
                println!("{} duplicate groups, {} reclaimable", groups.len(), human(wasted));
            });
        }
        Cmd::Analyze { root, top } => {
            let r = disk::analyze(&root, top);
            emit(j, &r, || {
                println!("{}: {} in {} files", r.root, human(r.bytes), r.files);
                for c in r.children.iter().take(top) {
                    println!("  {:>10}  {}{}", human(c.bytes), c.name, if c.is_dir { "/" } else { "" });
                }
                println!("Largest files:");
                for f in &r.largest_files {
                    println!("  {:>10}  {}", human(f.bytes), f.path);
                }
            });
        }
        Cmd::Shred { paths, passes, yes } => {
            if !yes {
                println!("This permanently destroys {} item(s). Re-run with --yes.", paths.len());
                return Ok(());
            }
            let r = shred::shred(&paths, passes);
            emit(j, &r, || println!("Shredded {} files ({}), {} failed", r.files, human(r.bytes), r.failed.len()));
        }
        Cmd::WipeFree { dir } => println!("{}", shred::wipe_free_space(&dir)?),
        Cmd::Drivers => {
            let d = drivers::list();
            emit(j, &d, || {
                for x in &d {
                    println!("{:<50} {:<18} {:<12} {}", x.device, x.version, x.date, x.provider);
                }
            });
        }
        Cmd::Serve { bind, read_only, new_token, show_token } => {
            if show_token {
                println!("{}", serve::token(new_token));
                return Ok(());
            }
            serve::run(&bind, read_only, new_token)?;
        }
        Cmd::Keys => {
            let k = keys::list();
            emit(j, &k, || {
                for x in &k {
                    println!("{:<10} {:<44} {}{}", x.kind, x.name, if x.value.is_empty() { "-" } else { &x.value }, if x.note.is_empty() { String::new() } else { format!("   ({})", x.note) });
                }
            });
        }
        Cmd::Report => println!("{}", serde_json::to_string_pretty(&toolkit::report())?),
        Cmd::Server => {
            let v = server::info();
            if j {
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("platform {}  kernel {}  load {}", v["platform"].as_str().unwrap_or(""), v["kernel"].as_str().unwrap_or(""), v["load"].as_str().unwrap_or(""));
                if let Some(g) = v["proxmox"]["guests"].as_array() {
                    println!("
Guests:");
                    for x in g {
                        println!("  {:>5} {:<5} {:<9} {}", x["vmid"], x["type"].as_str().unwrap_or(""), x["status"].as_str().unwrap_or(""), x["name"].as_str().unwrap_or(""));
                    }
                }
                if let Some(z) = v["zfs"].as_array().filter(|z| !z.is_empty()) {
                    println!("
ZFS:");
                    for p in z {
                        println!("  {:<14} {:<8} cap {}%  {}", p["name"].as_str().unwrap_or(""), p["health"].as_str().unwrap_or(""), p["cap"].as_str().unwrap_or(""), p["scan"].as_str().unwrap_or(""));
                    }
                }
                if let Some(d) = v["smart"].as_array().filter(|d| !d.is_empty()) {
                    println!("
SMART:");
                    for x in d {
                        println!("  {:<12} {:<34} passed={} temp={} hours={}", x["device"].as_str().unwrap_or(""), x["model"].as_str().unwrap_or(""), x["passed"], x["temp"], x["hours"]);
                    }
                }
                if let Some(f) = v["failed"].as_array().filter(|f| !f.is_empty()) {
                    println!("
Failed services: {}", f.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "));
                }
            }
        }
        Cmd::Repair { task, list } => {
            if list || task.is_none() {
                for t in toolkit::TASKS {
                    println!("{:<12} {:<34} {}{}", t.id, t.name, t.description, if t.admin { "  (admin)" } else { "" });
                }
                return Ok(());
            }
            let id = toolkit::run_task(task.as_deref().unwrap_or_default())?;
            let mut shown = 0;
            loop {
                let jb = jobs::get(id).expect("job");
                for l in jb.output.iter().skip(shown) {
                    println!("{l}");
                }
                shown = jb.output.len();
                if jb.state != "running" {
                    println!("-> {}: {}", jb.state, jb.message);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        Cmd::Top { n } => {
            let mut p = procs::list();
            p.truncate(n);
            emit(j, &p, || {
                for x in &p {
                    println!("{:>10}  {:>5.1}%  {:<7} {} ({})", human(x.memory), x.cpu, x.impact, x.name, x.pids.len());
                }
            });
        }
    }
    Ok(())
}
