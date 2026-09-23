//! Duplicate Finder: size -> first 64 KiB hash -> full BLAKE3 hash.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

use crate::fsutil;

#[derive(Debug, Clone, Serialize)]
pub struct DupFile {
    pub path: String,
    pub modified: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DupGroup {
    pub size: u64,
    pub hash: String,
    pub files: Vec<DupFile>,
    pub wasted: u64,
}

const SKIP_DIRS: &[&str] = &[
    "$recycle.bin", "system volume information", "windows", "windowsapps", "winsxs", "node_modules", ".git",
    "appdata", "programdata", "proc", "sys", "dev", "run", "snap", "library",
];

fn skip(p: &Path, include_system: bool) -> bool {
    if include_system {
        return false;
    }
    p.file_name()
        .map(|n| SKIP_DIRS.contains(&n.to_string_lossy().to_lowercase().as_str()))
        .unwrap_or(false)
}

fn hash_file(p: &Path, limit: Option<u64>) -> Option<String> {
    let mut f = File::open(p).ok()?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut left = limit.unwrap_or(u64::MAX);
    while left > 0 {
        let want = buf.len().min(left.min(usize::MAX as u64) as usize);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        left -= n as u64;
    }
    Some(h.finalize().to_hex().to_string())
}

pub fn find(roots: &[PathBuf], min_size: u64, include_system: bool) -> Vec<DupGroup> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for root in roots {
        let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| !(e.file_type().is_dir() && e.depth() > 0 && skip(e.path(), include_system)));
        for e in walker.flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            if let Ok(m) = e.metadata() {
                if m.len() >= min_size.max(1) {
                    by_size.entry(m.len()).or_default().push(e.into_path());
                }
            }
        }
    }
    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_, v)| v.len() > 1) {
        let mut by_head: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths {
            if let Some(h) = hash_file(&p, Some(64 * 1024)) {
                by_head.entry(h).or_default().push(p);
            }
        }
        for (_, cands) in by_head.into_iter().filter(|(_, v)| v.len() > 1) {
            let mut by_full: HashMap<String, Vec<PathBuf>> = HashMap::new();
            if size <= 64 * 1024 {
                // The head hash already covered the whole file.
                let h = hash_file(&cands[0], None).unwrap_or_default();
                by_full.insert(h, cands);
            } else {
                for p in cands {
                    if let Some(h) = hash_file(&p, None) {
                        by_full.entry(h).or_default().push(p);
                    }
                }
            }
            for (hash, same) in by_full.into_iter().filter(|(_, v)| v.len() > 1) {
                let mut files: Vec<DupFile> = same
                    .iter()
                    .map(|p| DupFile {
                        path: p.to_string_lossy().into(),
                        modified: p.metadata().map(|m| fsutil::modified_secs(&m)).unwrap_or(0),
                    })
                    .collect();
                files.sort_by_key(|f| f.modified);
                let wasted = size * (files.len() as u64 - 1);
                groups.push(DupGroup { size, hash, files, wasted });
            }
        }
    }
    groups.sort_by(|a, b| b.wasted.cmp(&a.wasted));
    groups.truncate(5000);
    groups
}

/// Duplicates go to the Recycle Bin / Trash, never straight to oblivion.
pub fn remove(paths: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut ok = Vec::new();
    let mut bad = Vec::new();
    for p in paths {
        match trash::delete(p) {
            Ok(()) => ok.push(p.clone()),
            Err(e) => bad.push((p.clone(), e.to_string())),
        }
    }
    (ok, bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_identical_files_only() {
        let root = std::env::temp_dir().join(format!("mwm-dupes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).unwrap();
        let big = vec![7u8; 100_000];
        std::fs::write(root.join("one.bin"), &big).unwrap();
        std::fs::write(root.join("a").join("two.bin"), &big).unwrap();
        let mut other = big.clone();
        other[99_999] = 1;
        std::fs::write(root.join("three.bin"), &other).unwrap();
        let g = find(&[root.clone()], 1, false);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].files.len(), 2);
        assert_eq!(g[0].wasted, 100_000);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
