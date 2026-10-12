//! Recycle-bin delete that also compiles on mobile.
//!
//! The `trash` crate has no Android/iOS target, so the dependency is gated to
//! desktop in Cargo.toml. On mobile (and any platform without a trash), this
//! falls back to a permanent delete — the Rescue/mobile surfaces never promise
//! an undoable Recycle Bin anyway.

use std::path::Path;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn delete<P: AsRef<Path>>(path: P) -> Result<(), String> {
    trash::delete(path).map_err(|e| e.to_string())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn delete<P: AsRef<Path>>(path: P) -> Result<(), String> {
    let path = path.as_ref();
    if path.is_dir() && !path.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}
