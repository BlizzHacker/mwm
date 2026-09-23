//! Commander: the dual-pane file manager engine (Double Commander / Geek
//! Squad FMOD style). Long operations run as `jobs` with progress + cancel.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use serde::Serialize;
use walkdir::WalkDir;

use crate::jobs::{self, Job};
use crate::{fsutil, util};

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
    pub hidden: bool,
    pub readonly: bool,
    pub link: bool,
    pub ext: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Root {
    pub path: String,
    pub label: String,
    pub kind: String,
    pub total: u64,
    pub free: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Listing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<Entry>,
    pub free: u64,
}

fn is_hidden(name: &str, _m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if _m.file_attributes() & (0x2 | 0x4) != 0 {
            return true;
        }
    }
    name.starts_with('.')
}

fn entry(path: &Path) -> Option<Entry> {
    let lm = fs::symlink_metadata(path).ok()?;
    let link = lm.file_type().is_symlink();
    let m = if link { fs::metadata(path).unwrap_or(lm) } else { lm };
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
    let is_dir = m.is_dir();
    let ext = if is_dir { String::new() } else { path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default() };
    Some(Entry {
        hidden: is_hidden(&name, &m),
        readonly: m.permissions().readonly(),
        size: if is_dir { 0 } else { m.len() },
        modified: fsutil::modified_secs(&m),
        path: path.to_string_lossy().into(),
        name,
        is_dir,
        link,
        ext,
    })
}

pub fn list(dir: &str) -> anyhow::Result<Listing> {
    let p = PathBuf::from(dir);
    let rd = fs::read_dir(&p).with_context(|| format!("cannot open {dir}"))?;
    let entries: Vec<Entry> = rd.flatten().filter_map(|e| entry(&e.path())).collect();
    let free = free_space(&p);
    Ok(Listing {
        path: p.to_string_lossy().into(),
        parent: p.parent().map(|x| x.to_string_lossy().to_string()).filter(|s| !s.is_empty()),
        entries,
        free,
    })
}

fn free_space(p: &Path) -> u64 {
    let s = p.to_string_lossy().to_lowercase();
    let mut best: Option<(usize, u64)> = None;
    for d in crate::sys::disks() {
        let m = d.mount.to_lowercase();
        if s.starts_with(&m) && best.map(|b| m.len() > b.0).unwrap_or(true) {
            best = Some((m.len(), d.free));
        }
    }
    best.map(|b| b.1).unwrap_or(0)
}

/// Drives + the usual quick places.
pub fn roots() -> Vec<Root> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let w: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
            let kind = match unsafe { GetDriveTypeW(w.as_ptr()) } {
                2 => "removable",
                3 => "fixed",
                4 => "network",
                5 => "optical",
                _ => "other",
            };
            let (mut free, mut total, mut _tf) = (0u64, 0u64, 0u64);
            let mut label = [0u16; 128];
            unsafe {
                GetDiskFreeSpaceExW(w.as_ptr(), &mut free, &mut total, &mut _tf);
                GetVolumeInformationW(w.as_ptr(), label.as_mut_ptr(), 128, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), 0);
            }
            let l = String::from_utf16_lossy(&label[..label.iter().position(|c| *c == 0).unwrap_or(0)]);
            v.push(Root { path: root, label: l, kind: kind.into(), total, free });
        }
    }
    #[cfg(not(windows))]
    {
        for d in crate::sys::disks() {
            v.push(Root { path: d.mount.clone(), label: d.name.clone(), kind: if d.removable { "removable".into() } else { "fixed".into() }, total: d.total, free: d.free });
        }
    }
    let h = util::home();
    for (label, p) in [("Home", h.clone()), ("Desktop", h.join("Desktop")), ("Documents", h.join("Documents")), ("Downloads", h.join("Downloads"))] {
        if p.is_dir() {
            v.push(Root { path: p.to_string_lossy().into(), label: label.into(), kind: "place".into(), total: 0, free: 0 });
        }
    }
    v
}

// ------------------------------------------------------------ simple ops ----

pub fn mkdir(parent: &str, name: &str) -> anyhow::Result<String> {
    if name.trim().is_empty() || name.split(['/', '\\']).any(|s| s == "..") {
        bail!("invalid name");
    }
    let p = Path::new(parent).join(name.trim());
    fs::create_dir_all(&p)?;
    Ok(p.to_string_lossy().into())
}

pub fn rename(path: &str, new_name: &str) -> anyhow::Result<String> {
    let nn = new_name.trim();
    if nn.is_empty() || nn.contains(['/', '\\']) {
        bail!("invalid name");
    }
    let p = Path::new(path);
    let to = p.parent().unwrap_or(Path::new(".")).join(nn);
    if to.exists() && !to.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy()) {
        bail!("{nn} already exists");
    }
    fs::rename(p, &to)?;
    Ok(to.to_string_lossy().into())
}

pub fn write_text(path: &str, text: &str) -> anyhow::Result<()> {
    fs::write(path, text)?;
    Ok(())
}

/// Names in `dest` that `sources` would collide with.
pub fn conflicts(sources: &[String], dest: &str) -> Vec<String> {
    sources
        .iter()
        .filter_map(|s| Path::new(s).file_name().map(|n| n.to_owned()))
        .filter(|n| Path::new(dest).join(n).exists())
        .map(|n| n.to_string_lossy().to_string())
        .collect()
}

// ------------------------------------------------------------- copy/move ----

fn unique_name(p: &Path) -> PathBuf {
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let parent = p.parent().unwrap_or(Path::new("."));
    for i in 2.. {
        let c = parent.join(format!("{stem} ({i}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    unreachable!()
}

fn copy_file(job: &Job, from: &Path, to: &Path) -> anyhow::Result<()> {
    let mut src = File::open(from).with_context(|| format!("open {}", from.display()))?;
    let meta = src.metadata()?;
    let tmp = to.with_extension(format!("{}mdwpart", to.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()));
    {
        let mut dst = File::create(&tmp).with_context(|| format!("create {}", to.display()))?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            if job.cancelled() {
                drop(dst);
                let _ = fs::remove_file(&tmp);
                bail!("cancelled");
            }
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])?;
            job.progress(n as u64, 0, "");
        }
        if let Ok(t) = meta.modified() {
            let _ = dst.set_modified(t);
        }
    }
    if to.exists() {
        fs::remove_file(to)?;
    }
    fs::rename(&tmp, to)?;
    if meta.permissions().readonly() {
        let _ = fs::set_permissions(to, meta.permissions());
    }
    job.progress(0, 1, "");
    Ok(())
}

fn resolve_target(src: &Path, dest_dir: &Path, mode: &str) -> Option<PathBuf> {
    let t = dest_dir.join(src.file_name()?);
    if !t.exists() {
        return Some(t);
    }
    match mode {
        "skip" => None,
        "rename" => Some(unique_name(&t)),
        _ => Some(t), // overwrite
    }
}

fn plan(sources: &[String]) -> (u64, u64) {
    let (mut b, mut n) = (0, 0);
    for s in sources {
        for e in WalkDir::new(s).follow_links(false).into_iter().flatten() {
            if e.file_type().is_file() {
                b += e.metadata().map(|m| m.len()).unwrap_or(0);
                n += 1;
            }
        }
    }
    (b, n)
}

fn copy_tree(job: &Job, src: &Path, target: &Path, errors: &mut Vec<String>) {
    if src.is_dir() {
        if target.starts_with(src) {
            errors.push(format!("{}: can't copy a folder into itself", src.display()));
            return;
        }
        for e in WalkDir::new(src).follow_links(false).into_iter().flatten() {
            if job.cancelled() {
                return;
            }
            let rel = e.path().strip_prefix(src).unwrap_or(e.path());
            let t = target.join(rel);
            if e.file_type().is_dir() {
                if let Err(err) = fs::create_dir_all(&t) {
                    errors.push(format!("{}: {err}", t.display()));
                }
            } else if e.file_type().is_file() {
                job.current(&e.path().to_string_lossy());
                if let Err(err) = copy_file(job, e.path(), &t) {
                    errors.push(format!("{}: {err}", e.path().display()));
                }
            }
        }
    } else {
        job.current(&src.to_string_lossy());
        if let Err(err) = copy_file(job, src, target) {
            errors.push(format!("{}: {err}", src.display()));
        }
    }
}

fn summary(verb: &str, n: usize, errors: &[String], job: &Job) -> anyhow::Result<String> {
    for e in errors.iter().take(200) {
        job.line(e.clone());
    }
    if job.cancelled() {
        bail!("cancelled");
    }
    if errors.is_empty() {
        Ok(format!("{verb} {n} item{}", if n == 1 { "" } else { "s" }))
    } else {
        bail!("{verb} with {} error{} - see details", errors.len(), if errors.len() == 1 { "" } else { "s" })
    }
}

/// Copy (or move) `sources` into `dest`. `mode`: overwrite | skip | rename.
pub fn transfer(sources: Vec<String>, dest: String, mode: String, is_move: bool) -> u64 {
    let title = format!("{} {} item{} to {}", if is_move { "Move" } else { "Copy" }, sources.len(), if sources.len() == 1 { "" } else { "s" }, dest);
    jobs::start(if is_move { "move" } else { "copy" }, &title, "files", move |job| {
        let dest_dir = PathBuf::from(&dest);
        if !dest_dir.is_dir() {
            bail!("{dest} is not a folder");
        }
        let (bytes, items) = plan(&sources);
        job.set_total(bytes, items);
        let mut errors = Vec::new();
        for s in &sources {
            if job.cancelled() {
                break;
            }
            let src = PathBuf::from(s);
            let Some(target) = resolve_target(&src, &dest_dir, &mode) else { continue };
            if src == target {
                continue;
            }
            if is_move {
                if target.exists() && mode == "overwrite" {
                    let _ = if target.is_dir() { fs::remove_dir_all(&target) } else { fs::remove_file(&target) };
                }
                // Same volume: instant rename. Otherwise copy then delete.
                if fs::rename(&src, &target).is_ok() {
                    let (b, n) = plan(&[target.to_string_lossy().into()]);
                    job.progress(b, n, &target.to_string_lossy());
                    continue;
                }
                let before = errors.len();
                copy_tree(job, &src, &target, &mut errors);
                if errors.len() == before && !job.cancelled() {
                    let r = if src.is_dir() { fs::remove_dir_all(&src) } else { fs::remove_file(&src) };
                    if let Err(e) = r {
                        errors.push(format!("copied but could not remove {}: {e}", src.display()));
                    }
                }
            } else {
                copy_tree(job, &src, &target, &mut errors);
            }
        }
        summary(if is_move { "Moved" } else { "Copied" }, sources.len(), &errors, job)
    })
}

/// Delete to Recycle Bin / Trash, or permanently.
pub fn delete(paths: Vec<String>, permanent: bool) -> u64 {
    let title = format!("{} {} item{}", if permanent { "Delete" } else { "Recycle" }, paths.len(), if paths.len() == 1 { "" } else { "s" });
    jobs::start("delete", &title, "files", move |job| {
        job.set_total(0, paths.len() as u64);
        let mut errors = Vec::new();
        for p in &paths {
            if job.cancelled() {
                break;
            }
            job.current(p);
            let r = if permanent {
                let pp = Path::new(p);
                if pp.is_dir() { fs::remove_dir_all(pp).map_err(|e| e.to_string()) } else { fs::remove_file(pp).map_err(|e| e.to_string()) }
            } else {
                trash::delete(p).map_err(|e| e.to_string())
            };
            match r {
                Ok(()) => job.progress(0, 1, ""),
                Err(e) => errors.push(format!("{p}: {e}")),
            }
        }
        summary(if permanent { "Deleted" } else { "Recycled" }, paths.len(), &errors, job)
    })
}

// ------------------------------------------------------------ archives ----

/// Zip `sources` (all from one folder) into `archive`.
pub fn pack(sources: Vec<String>, archive: String) -> u64 {
    jobs::start("pack", &format!("Pack {}", archive), "files", move |job| {
        let first = PathBuf::from(sources.first().context("nothing to pack")?);
        let parent = first.parent().context("no parent folder")?.to_path_buf();
        let names: Vec<String> = sources.iter().filter_map(|s| Path::new(s).file_name().map(|n| n.to_string_lossy().to_string())).collect();
        job.current(&archive);
        let (code, out) = if cfg!(windows) {
            let mut args = vec!["-a".to_string(), "-c".into(), "-f".into(), archive.clone(), "-C".into(), parent.to_string_lossy().into()];
            args.extend(names);
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            util::run_capture("tar.exe", &a)?
        } else {
            let mut c = util::cmd("zip");
            c.current_dir(&parent).arg("-r").arg(&archive).args(&names);
            let o = c.output().context("the `zip` command is not installed")?;
            (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stderr).into())
        };
        if code != 0 {
            bail!("pack failed: {}", out.trim());
        }
        Ok(format!("Created {archive}"))
    })
}

/// Extract `archive` (zip, tar, tar.gz, ...) into `dest`.
pub fn unpack(archive: String, dest: String) -> u64 {
    jobs::start("unpack", &format!("Unpack {}", archive), "files", move |job| {
        fs::create_dir_all(&dest)?;
        job.current(&archive);
        let lower = archive.to_lowercase();
        let (code, out) = if cfg!(windows) || !lower.ends_with(".zip") {
            util::run_capture(if cfg!(windows) { "tar.exe" } else { "tar" }, &["-xf", &archive, "-C", &dest])?
        } else {
            util::run_capture("unzip", &["-o", &archive, "-d", &dest])?
        };
        if code != 0 {
            bail!("unpack failed: {}", out.trim());
        }
        Ok(format!("Extracted to {dest}"))
    })
}

// -------------------------------------------------------------- search ----

/// Find files whose name matches `pattern` (glob or substring), optionally
/// containing `text`. Hits stream into the job's output.
pub fn search(root: String, pattern: String, text: String) -> u64 {
    jobs::start("search", &format!("Search \"{}\" in {}", if pattern.is_empty() { &text } else { &pattern }, root), "files", move |job| {
        let pat = pattern.trim().to_lowercase();
        let glob = if pat.is_empty() { "*".to_string() } else if pat.contains(['*', '?']) { pat } else { format!("*{pat}*") };
        let needle = text.to_lowercase();
        let mut hits = 0u64;
        for e in WalkDir::new(&root).follow_links(false).into_iter().flatten() {
            if job.cancelled() {
                break;
            }
            let name = e.file_name().to_string_lossy();
            if !fsutil::glob_match(&glob, &name) {
                continue;
            }
            if !needle.is_empty() {
                if !e.file_type().is_file() || e.metadata().map(|m| m.len() > 50 << 20).unwrap_or(true) {
                    continue;
                }
                let Ok(bytes) = fs::read(e.path()) else { continue };
                if !String::from_utf8_lossy(&bytes).to_lowercase().contains(&needle) {
                    continue;
                }
            }
            hits += 1;
            job.line(e.path().to_string_lossy().to_string());
            job.progress(0, 1, &e.path().to_string_lossy());
            if hits >= 5000 {
                job.line("... stopped at 5000 results");
                break;
            }
        }
        Ok(format!("{hits} found"))
    })
}

// ------------------------------------------------------ preview / props ----

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    /// text | image | binary | dir
    pub kind: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}

pub fn preview(path: &str) -> anyhow::Result<Preview> {
    let p = Path::new(path);
    let m = fs::metadata(p)?;
    if m.is_dir() {
        let n = fs::read_dir(p).map(|r| r.count()).unwrap_or(0);
        return Ok(Preview { kind: "dir".into(), content: format!("{n} items"), size: 0, truncated: false });
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    };
    if let Some(mime) = mime {
        if m.len() <= 20 << 20 {
            return Ok(Preview { kind: "image".into(), content: format!("data:{mime};base64,{}", base64(&fs::read(p)?)), size: m.len(), truncated: false });
        }
    }
    let mut buf = Vec::new();
    File::open(p)?.take(512 * 1024).read_to_end(&mut buf)?;
    let truncated = m.len() > buf.len() as u64;
    let head = &buf[..buf.len().min(8192)];
    let binary = head.contains(&0) && !(head.len() > 2 && (head[0] == 0xFF && head[1] == 0xFE));
    if binary {
        let mut out = String::new();
        for (i, row) in buf.chunks(16).take(256).enumerate() {
            let hex: Vec<String> = row.iter().map(|b| format!("{b:02x}")).collect();
            let asc: String = row.iter().map(|b| if b.is_ascii_graphic() || *b == b' ' { *b as char } else { '.' }).collect();
            out.push_str(&format!("{:08x}  {:<48} {}\n", i * 16, hex.join(" "), asc));
        }
        return Ok(Preview { kind: "binary".into(), content: out, size: m.len(), truncated: true });
    }
    let text = if buf.len() >= 2 && buf[0] == 0xFF && buf[1] == 0xFE {
        let u: Vec<u16> = buf[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&buf).to_string()
    };
    Ok(Preview { kind: "text".into(), content: text, size: m.len(), truncated })
}

#[derive(Debug, Clone, Serialize)]
pub struct Props {
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
    pub readonly: bool,
    pub hidden: bool,
}

fn secs(t: std::io::Result<std::time::SystemTime>) -> u64 {
    t.ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn properties(path: &str) -> anyhow::Result<Props> {
    let p = Path::new(path);
    let m = fs::metadata(p)?;
    let (mut size, mut files, mut dirs) = (m.len(), 1, 0);
    if m.is_dir() {
        size = 0;
        files = 0;
        for e in WalkDir::new(p).follow_links(false).min_depth(1).into_iter().flatten() {
            if e.file_type().is_dir() {
                dirs += 1;
            } else if e.file_type().is_file() {
                files += 1;
                size += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    Ok(Props {
        path: path.into(),
        is_dir: m.is_dir(),
        size,
        files,
        dirs,
        created: secs(m.created()),
        modified: secs(m.modified()),
        accessed: secs(m.accessed()),
        readonly: m.permissions().readonly(),
        hidden: is_hidden(&name, &m),
    })
}

/// Folder sizes for the "calculate sizes" action.
pub fn dir_sizes(paths: &[String]) -> Vec<(String, u64)> {
    paths.iter().map(|p| (p.clone(), fsutil::tree_size(Path::new(p)).bytes)).collect()
}

/// Apply a multi-rename plan: (old path, new name). Validates first, then renames.
pub fn multi_rename(plan: &[(String, String)]) -> anyhow::Result<usize> {
    let mut seen = std::collections::HashSet::new();
    for (old, new) in plan {
        if new.trim().is_empty() || new.contains(['/', '\\']) {
            bail!("invalid name: {new:?}");
        }
        let parent = Path::new(old).parent().unwrap_or(Path::new("."));
        if !seen.insert(parent.join(new).to_string_lossy().to_lowercase()) {
            bail!("two files would be named {new}");
        }
    }
    // Two-phase so swaps like a->b, b->a work.
    let mut staged = Vec::new();
    for (i, (old, new)) in plan.iter().enumerate() {
        let p = Path::new(old);
        let tmp = p.with_file_name(format!(".mdw-rename-{i}-{}", util::now_secs()));
        fs::rename(p, &tmp).with_context(|| format!("rename {old}"))?;
        staged.push((tmp, p.with_file_name(new)));
    }
    let mut n = 0;
    for (tmp, to) in staged {
        let to = if to.exists() { unique_name(&to) } else { to };
        fs::rename(&tmp, &to)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mdw-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn copy_move_conflicts_and_rename() {
        let a = tmp("a");
        let b = tmp("b");
        fs::create_dir_all(a.join("dir/sub")).unwrap();
        fs::write(a.join("dir/sub/x.txt"), b"hello").unwrap();
        fs::write(a.join("f.txt"), vec![3u8; 3_000_000]).unwrap();
        let srcs = vec![a.join("dir").to_string_lossy().to_string(), a.join("f.txt").to_string_lossy().to_string()];
        let j = jobs::wait(transfer(srcs.clone(), b.to_string_lossy().into(), "skip".into(), false)).unwrap();
        assert_eq!(j.state, "done", "{:?}", j.output);
        assert_eq!(j.done_bytes, 3_000_005);
        assert_eq!(fs::read(b.join("dir/sub/x.txt")).unwrap(), b"hello");
        assert_eq!(conflicts(&srcs, &b.to_string_lossy()).len(), 2);
        // rename mode creates "f (2).txt"
        jobs::wait(transfer(vec![srcs[1].clone()], b.to_string_lossy().into(), "rename".into(), false));
        assert!(b.join("f (2).txt").exists());
        // move
        let c = tmp("c");
        let j = jobs::wait(transfer(vec![srcs[0].clone()], c.to_string_lossy().into(), "overwrite".into(), true)).unwrap();
        assert_eq!(j.state, "done");
        assert!(!a.join("dir").exists() && c.join("dir/sub/x.txt").exists());
        // multi-rename with a swap
        fs::write(c.join("1.txt"), b"1").unwrap();
        fs::write(c.join("2.txt"), b"2").unwrap();
        let plan = vec![(c.join("1.txt").to_string_lossy().to_string(), "2.txt".to_string()), (c.join("2.txt").to_string_lossy().to_string(), "1.txt".to_string())];
        assert_eq!(multi_rename(&plan).unwrap(), 2);
        assert_eq!(fs::read(c.join("2.txt")).unwrap(), b"1");
        assert_eq!(list(&c.to_string_lossy()).unwrap().entries.len(), 3);
        for d in [a, b, c] {
            fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn b64() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }
}
