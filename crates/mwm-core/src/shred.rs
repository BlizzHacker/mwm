//! Unrecoverable Delete (file shredder) and free-space wiping (Revo's
//! "Evidence Remover").
//!
//! Overwriting is meaningful on spinning disks. On SSDs wear-levelling means
//! old blocks may survive; the UI says so and recommends free-space wiping +
//! TRIM as well.

use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Report {
    pub files: u64,
    pub bytes: u64,
    pub failed: Vec<(String, String)>,
}

struct XorShift(u64);

impl XorShift {
    fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            let b = self.0.to_le_bytes();
            chunk.copy_from_slice(&b[..chunk.len()]);
        }
    }
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    t ^ 0x9E37_79B9_7F4A_7C15 ^ (std::process::id() as u64) << 32
}

fn shred_file(p: &Path, passes: u32) -> std::io::Result<u64> {
    let meta = fs::metadata(p)?;
    let len = meta.len();
    if meta.permissions().readonly() {
        let mut perms = meta.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(p, perms)?;
    }
    {
        let mut f = OpenOptions::new().write(true).open(p)?;
        let mut rng = XorShift(seed() | 1);
        let mut buf = vec![0u8; 1 << 20];
        for pass in 0..=passes {
            f.seek(SeekFrom::Start(0))?;
            let last = pass == passes;
            if last {
                buf.iter_mut().for_each(|b| *b = 0);
            }
            let mut left = len;
            while left > 0 {
                let n = buf.len().min(left as usize);
                if !last {
                    rng.fill(&mut buf[..n]);
                }
                f.write_all(&buf[..n])?;
                left -= n as u64;
            }
            f.sync_all()?;
        }
        f.set_len(0)?;
        f.sync_all()?;
    }
    // Scrub the name too before unlinking.
    let parent = p.parent().unwrap_or(Path::new("."));
    let mut rng = XorShift(seed() | 1);
    let mut nb = [0u8; 8];
    rng.fill(&mut nb);
    let tmp: PathBuf = parent.join(format!("{:016x}.mwm", u64::from_le_bytes(nb)));
    let target = if fs::rename(p, &tmp).is_ok() { tmp } else { p.to_path_buf() };
    fs::remove_file(&target)?;
    Ok(len)
}

pub fn shred(paths: &[String], passes: u32) -> Report {
    let mut rep = Report::default();
    for raw in paths {
        let p = Path::new(raw);
        if p.is_dir() {
            for e in WalkDir::new(p).follow_links(false).contents_first(true).into_iter().flatten() {
                if e.file_type().is_file() {
                    match shred_file(e.path(), passes) {
                        Ok(n) => {
                            rep.files += 1;
                            rep.bytes += n;
                        }
                        Err(err) => rep.failed.push((e.path().to_string_lossy().into(), err.to_string())),
                    }
                } else if e.file_type().is_dir() {
                    let _ = fs::remove_dir(e.path());
                }
            }
        } else {
            match shred_file(p, passes) {
                Ok(n) => {
                    rep.files += 1;
                    rep.bytes += n;
                }
                Err(err) => rep.failed.push((raw.clone(), err.to_string())),
            }
        }
    }
    rep
}

/// Overwrite the free space of the volume holding `dir`.
pub fn wipe_free_space(dir: &str) -> anyhow::Result<String> {
    #[cfg(windows)]
    {
        let (code, out) = crate::util::run_capture("cipher.exe", &[&format!("/w:{dir}")])?;
        let tail: Vec<&str> = out.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('.')).collect();
        if code != 0 {
            anyhow::bail!("cipher exited with {code}: {}", tail.join(" "));
        }
        Ok(tail.last().map(|s| s.to_string()).unwrap_or_else(|| "Free space wiped.".into()))
    }
    #[cfg(not(windows))]
    {
        let file = Path::new(dir).join(".mwm-wipe.tmp");
        let mut written = 0u64;
        {
            let mut f = fs::File::create(&file)?;
            let buf = vec![0u8; 4 << 20];
            loop {
                match f.write_all(&buf) {
                    Ok(()) => written += buf.len() as u64,
                    Err(_) => break,
                }
            }
            let _ = f.sync_all();
        }
        fs::remove_file(&file)?;
        Ok(format!("Overwrote {} of free space.", written))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shreds_and_removes() {
        let dir = std::env::temp_dir().join(format!("mwm-shred-{}", std::process::id()));
        fs::create_dir_all(dir.join("inner")).unwrap();
        fs::write(dir.join("secret.txt"), b"top secret").unwrap();
        fs::write(dir.join("inner").join("b.txt"), vec![1u8; 3_000_000]).unwrap();
        let r = shred(&[dir.to_string_lossy().into()], 1);
        assert_eq!(r.files, 2);
        assert_eq!(r.bytes, 3_000_010);
        assert!(r.failed.is_empty());
        assert!(!dir.exists());
    }
}
