//! MDW - Move Digital Weight.
//!
//! The engine behind the desktop app and the `mdw` CLI. Every module exposes
//! the same API on every OS; platform differences live behind `cfg` inside
//! each module, so the UI never has to care where it is running.

pub mod apps;
pub mod cleaner;
pub mod disk;
pub mod drivers;
pub mod dupes;
pub mod files;
pub mod jobs;
pub mod fsutil;
pub mod procs;
pub mod shred;
pub mod startup;
pub mod sys;
pub mod toolkit;
pub mod updater;
pub mod util;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
