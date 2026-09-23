//! Disk Analyzer: what is actually heavy on a drive or in a folder.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub root: String,
    pub bytes: u64,
    pub files: u64,
    pub children: Vec<Entry>,
    pub largest_files: Vec<Entry>,
    pub by_type: Vec<(String, u64)>,
}

pub fn analyze(root: &Path, top: usize) -> Report {
    let mut children: HashMap<PathBuf, (u64, u64, bool)> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut types: HashMap<String, u64> = HashMap::new();
    let (mut bytes, mut files) = (0u64, 0u64);

    for e in WalkDir::new(root).follow_links(false).min_depth(1).into_iter().flatten() {
        let rel_first = e.path().strip_prefix(root).ok().and_then(|r| r.components().next()).map(|c| root.join(c.as_os_str()));
        if e.depth() == 1 {
            children.entry(e.path().to_path_buf()).or_insert((0, 0, e.file_type().is_dir()));
        }
        if !e.file_type().is_file() {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        let len = m.len();
        bytes += len;
        files += 1;
        if let Some(first) = rel_first {
            let c = children.entry(first).or_insert((0, 0, false));
            c.0 += len;
            c.1 += 1;
        }
        let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_else(|| "(none)".into());
        *types.entry(ext).or_default() += len;
        heap.push(Reverse((len, e.into_path())));
        if heap.len() > top {
            heap.pop();
        }
    }

    let mut kids: Vec<Entry> = children
        .into_iter()
        .map(|(p, (b, f, d))| Entry {
            name: p.file_name().unwrap_or_default().to_string_lossy().into(),
            path: p.to_string_lossy().into(),
            bytes: b,
            files: f,
            is_dir: d,
        })
        .collect();
    kids.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let mut largest: Vec<Entry> = heap
        .into_iter()
        .map(|Reverse((b, p))| Entry {
            name: p.file_name().unwrap_or_default().to_string_lossy().into(),
            path: p.to_string_lossy().into(),
            bytes: b,
            files: 1,
            is_dir: false,
        })
        .collect();
    largest.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    let mut by_type: Vec<(String, u64)> = types.into_iter().collect();
    by_type.sort_by(|a, b| b.1.cmp(&a.1));
    by_type.truncate(15);

    Report { root: root.to_string_lossy().into(), bytes, files, children: kids, largest_files: largest, by_type }
}
