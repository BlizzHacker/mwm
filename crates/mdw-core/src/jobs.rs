//! Background jobs (file copies, repairs, scans). They run on their own
//! threads, report progress into a shared table and survive whatever the UI is
//! doing - the UI just polls `list()`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct JobInfo {
    pub id: u64,
    /// copy | move | delete | pack | unpack | search | repair | ...
    pub kind: String,
    pub title: String,
    /// Which UI page owns this job (for the activity panel link).
    pub page: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub done_items: u64,
    pub total_items: u64,
    pub current: String,
    /// running | done | failed | cancelled
    pub state: String,
    pub message: String,
    /// Captured command output / result lines (repairs, search hits...).
    pub output: Vec<String>,
    pub started: u64,
    pub finished: u64,
}

struct Table {
    jobs: Vec<JobInfo>,
    cancel: HashMap<u64, Arc<AtomicBool>>,
}

fn table() -> &'static Mutex<Table> {
    static T: OnceLock<Mutex<Table>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Table { jobs: Vec::new(), cancel: HashMap::new() }))
}

static NEXT: AtomicU64 = AtomicU64::new(1);

/// Handed to the job body so it can report progress and notice cancellation.
#[derive(Clone)]
pub struct Job {
    pub id: u64,
    cancel: Arc<AtomicBool>,
}

impl Job {
    fn with<F: FnOnce(&mut JobInfo)>(&self, f: F) {
        if let Ok(mut t) = table().lock() {
            if let Some(j) = t.jobs.iter_mut().find(|j| j.id == self.id) {
                f(j);
            }
        }
    }
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn set_total(&self, bytes: u64, items: u64) {
        self.with(|j| {
            j.total_bytes = bytes;
            j.total_items = items;
        });
    }
    pub fn progress(&self, bytes: u64, items: u64, current: &str) {
        self.with(|j| {
            j.done_bytes += bytes;
            j.done_items += items;
            if !current.is_empty() {
                j.current = current.to_string();
            }
        });
    }
    pub fn line(&self, s: impl Into<String>) {
        let s = s.into();
        self.with(|j| {
            j.output.push(s);
            if j.output.len() > 4000 {
                j.output.drain(0..1000);
            }
        });
    }
    pub fn current(&self, s: &str) {
        self.with(|j| j.current = s.to_string());
    }
}

/// Start `body` on a new thread. Its `Ok(msg)` / `Err(e)` becomes the final state.
pub fn start<F>(kind: &str, title: &str, page: &str, body: F) -> u64
where
    F: FnOnce(&Job) -> anyhow::Result<String> + Send + 'static,
{
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut t) = table().lock() {
        t.jobs.push(JobInfo {
            id,
            kind: kind.into(),
            title: title.into(),
            page: page.into(),
            state: "running".into(),
            started: crate::util::now_secs(),
            ..Default::default()
        });
        t.cancel.insert(id, cancel.clone());
    }
    let job = Job { id, cancel };
    std::thread::spawn(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&job)));
        let cancelled = job.cancelled();
        job.with(|j| {
            j.finished = crate::util::now_secs();
            match res {
                Ok(Ok(msg)) => {
                    j.state = if cancelled { "cancelled".into() } else { "done".into() };
                    j.message = msg;
                }
                Ok(Err(e)) => {
                    j.state = if cancelled { "cancelled".into() } else { "failed".into() };
                    j.message = e.to_string();
                }
                Err(_) => {
                    j.state = "failed".into();
                    j.message = "internal error".into();
                }
            }
        });
    });
    id
}

pub fn list() -> Vec<JobInfo> {
    table().lock().map(|t| t.jobs.clone()).unwrap_or_default()
}

pub fn get(id: u64) -> Option<JobInfo> {
    table().lock().ok()?.jobs.iter().find(|j| j.id == id).cloned()
}

pub fn cancel(id: u64) {
    if let Ok(t) = table().lock() {
        if let Some(c) = t.cancel.get(&id) {
            c.store(true, Ordering::Relaxed);
        }
    }
}

/// Forget finished jobs (keeps running ones).
pub fn clear_finished() {
    if let Ok(mut t) = table().lock() {
        t.jobs.retain(|j| j.state == "running");
        let live: Vec<u64> = t.jobs.iter().map(|j| j.id).collect();
        t.cancel.retain(|k, _| live.contains(k));
    }
}

/// Block until a job ends (CLI use).
pub fn wait(id: u64) -> Option<JobInfo> {
    loop {
        let j = get(id)?;
        if j.state != "running" {
            return Some(j);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_lifecycle_and_cancel() {
        let id = start("t", "test", "x", |j| {
            j.set_total(10, 2);
            j.progress(5, 1, "a");
            Ok("fine".into())
        });
        let done = wait(id).unwrap();
        assert_eq!((done.state.as_str(), done.done_bytes, done.message.as_str()), ("done", 5, "fine"));

        let id = start("t", "slow", "x", |j| {
            while !j.cancelled() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            anyhow::bail!("stopped")
        });
        cancel(id);
        assert_eq!(wait(id).unwrap().state, "cancelled");
    }
}
