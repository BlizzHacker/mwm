//! `mdw` - Move Digital Weight from the command line (servers, SSH, scripts).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use mdw_core::{apps, cleaner, disk, drivers, dupes, procs, shred, startup, sys, updater};

#[derive(Parser)]
#[command(name = "mdw", version, about = "MDW - Move Digital Weight: clean junk, uninstall cleanly, tame startup.")]
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
        /// Comma-separated rule ids (see `mdw scan`).
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
    /// Uninstall a program by id (see `mdw apps --json`), then list leftovers.
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
                println!("MDW {} on {} ({}){}", i.version, i.hostname, i.os, if i.elevated { " [admin]" } else { "" });
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
                println!("Moved {} of digital weight.", human(total));
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
