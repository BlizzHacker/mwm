use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Tally {
    pub bytes: u64,
    pub files: u64,
}

impl Tally {
    pub fn add(&mut self, o: Tally) {
        self.bytes += o.bytes;
        self.files += o.files;
    }
}

/// Which files inside a target directory are fair game.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub min_age: Option<Duration>,
    /// File-name globs (`*.log`, `thumbcache_*.db`). Empty = every file.
    pub patterns: Vec<String>,
    /// Only look at the directory itself, not below it.
    pub shallow: bool,
}

impl Filter {
    fn accepts(&self, name: &str, meta: &Metadata) -> bool {
        if !self.patterns.is_empty() && !self.patterns.iter().any(|p| glob_match(p, name)) {
            return false;
        }
        match self.min_age {
            None => true,
            Some(age) => meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .map(|a| a >= age)
                .unwrap_or(true),
        }
    }
}

/// Case-insensitive `*` / `?` glob against a single file name.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Expand `*` wildcards inside path segments into the paths that exist.
pub fn expand(pattern: &Path) -> Vec<PathBuf> {
    let mut acc = vec![PathBuf::new()];
    for comp in pattern.components() {
        let seg = comp.as_os_str().to_string_lossy();
        if matches!(comp, Component::Normal(_)) && (seg.contains('*') || seg.contains('?')) {
            let mut next = Vec::new();
            for base in &acc {
                if let Ok(rd) = fs::read_dir(base) {
                    for e in rd.flatten() {
                        if glob_match(&seg, &e.file_name().to_string_lossy()) {
                            next.push(e.path());
                        }
                    }
                }
            }
            acc = next;
        } else {
            for a in acc.iter_mut() {
                a.push(comp.as_os_str());
            }
        }
    }
    acc.into_iter().filter(|p| p.exists()).collect()
}

fn walker(dir: &Path, f: &Filter) -> WalkDir {
    let w = WalkDir::new(dir).follow_links(false).min_depth(1);
    if f.shallow {
        w.max_depth(1)
    } else {
        w
    }
}

/// Total size of the files under `dir` that `filter` would clean.
pub fn measure(dir: &Path, filter: &Filter) -> Tally {
    let mut t = Tally::default();
    if dir.is_file() {
        if let Ok(m) = dir.metadata() {
            t.bytes = m.len();
            t.files = 1;
        }
        return t;
    }
    for e in walker(dir, filter).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if filter.accepts(&e.file_name().to_string_lossy(), &m) {
                t.bytes += m.len();
                t.files += 1;
            }
        }
    }
    t
}

/// Plain size of a whole tree (no filter).
pub fn tree_size(path: &Path) -> Tally {
    measure(path, &Filter::default())
}

fn remove_file_forced(p: &Path) -> std::io::Result<()> {
    match fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Read-only files refuse deletion on Windows; clear the flag once.
            if let Ok(m) = p.metadata() {
                let mut perms = m.permissions();
                if perms.readonly() {
                    #[allow(clippy::permissions_set_readonly_false)]
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(p, perms);
                    return fs::remove_file(p);
                }
            }
            Err(e)
        }
    }
}

/// Delete what `filter` selects below `dir`, keeping `dir` itself. Files that
/// are locked or protected are skipped and counted as errors, never fatal.
pub fn delete_contents(dir: &Path, filter: &Filter) -> (Tally, u64) {
    let mut freed = Tally::default();
    let mut errors = 0u64;
    if dir.is_file() {
        let len = dir.metadata().map(|m| m.len()).unwrap_or(0);
        match remove_file_forced(dir) {
            Ok(()) => {
                freed.bytes += len;
                freed.files += 1;
            }
            Err(_) => errors += 1,
        }
        return (freed, errors);
    }
    let prune_dirs = filter.patterns.is_empty() && !filter.shallow;
    for e in walker(dir, filter).contents_first(true).into_iter().flatten() {
        let ft = e.file_type();
        if ft.is_dir() {
            if prune_dirs {
                // Only succeeds when empty - exactly what we want.
                let _ = fs::remove_dir(e.path());
            }
            continue;
        }
        if ft.is_symlink() {
            if prune_dirs {
                let _ = fs::remove_file(e.path()).or_else(|_| fs::remove_dir(e.path()));
            }
            continue;
        }
        let Ok(m) = e.metadata() else {
            errors += 1;
            continue;
        };
        if !filter.accepts(&e.file_name().to_string_lossy(), &m) {
            continue;
        }
        match remove_file_forced(e.path()) {
            Ok(()) => {
                freed.bytes += m.len();
                freed.files += 1;
            }
            Err(_) => errors += 1,
        }
    }
    (freed, errors)
}

pub fn modified_secs(m: &Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globbing() {
        assert!(glob_match("*.log", "CBS.LOG"));
        assert!(glob_match("thumbcache_*.db", "thumbcache_256.db"));
        assert!(!glob_match("*.log", "log.txt"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*@*.journal", "system@abc-123.journal"));
        assert!(!glob_match("*@*.journal", "system.journal"));
    }

    #[test]
    fn deletes_only_matching_and_keeps_root() {
        let root = std::env::temp_dir().join(format!("mwm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.log"), b"12345").unwrap();
        fs::write(root.join("keep.txt"), b"1").unwrap();
        fs::write(root.join("sub").join("b.log"), b"123").unwrap();
        let f = Filter { patterns: vec!["*.log".into()], ..Default::default() };
        assert_eq!(measure(&root, &f).bytes, 8);
        let (freed, errs) = delete_contents(&root, &f);
        assert_eq!((freed.bytes, freed.files, errs), (8, 2, 0));
        assert!(root.join("keep.txt").exists());
        let (all, _) = delete_contents(&root, &Filter::default());
        assert_eq!(all.files, 1);
        assert!(root.exists() && !root.join("sub").exists());
        fs::remove_dir_all(&root).unwrap();
    }
}
