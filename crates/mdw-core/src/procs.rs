//! Performance: what is running and how heavy it is.

use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize)]
pub struct Proc {
    pub name: String,
    pub pids: Vec<u32>,
    pub memory: u64,
    pub cpu: f32,
    pub exe: String,
    pub impact: String,
}

/// Processes grouped by executable name, heaviest first.
pub fn list() -> Vec<Proc> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250)));
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let cores = sys.cpus().len().max(1) as f32;
    let mut groups: std::collections::HashMap<String, Proc> = std::collections::HashMap::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().to_string();
        if name.is_empty() || pid.as_u32() <= 4 {
            continue;
        }
        let g = groups.entry(name.to_lowercase()).or_insert_with(|| Proc {
            name: name.clone(),
            pids: vec![],
            memory: 0,
            cpu: 0.0,
            exe: p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(),
            impact: String::new(),
        });
        g.pids.push(pid.as_u32());
        g.memory += p.memory();
        g.cpu += p.cpu_usage() / cores;
    }
    let mut v: Vec<Proc> = groups.into_values().collect();
    for p in v.iter_mut() {
        let mb = p.memory as f64 / 1_048_576.0;
        p.impact = if mb > 1024.0 || p.cpu > 15.0 {
            "High"
        } else if mb > 300.0 || p.cpu > 3.0 {
            "Medium"
        } else {
            "Low"
        }
        .into();
    }
    v.sort_by(|a, b| b.memory.cmp(&a.memory));
    v
}

pub fn kill(pids: &[u32]) -> usize {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    pids.iter().filter(|p| sys.process(Pid::from_u32(**p)).map(|pr| pr.kill()).unwrap_or(false)).count()
}
