//! MWM - Move Weight Manager.
//!
//! The engine behind the desktop app and the `mwm` CLI. Every module exposes
//! the same API on every OS; platform differences live behind `cfg` inside
//! each module, so the UI never has to care where it is running.

pub mod api;
pub mod arkana;
pub mod apps;
pub mod cleaner;
pub mod disk;
pub mod drivers;
pub mod dupes;
pub mod files;
pub mod jobs;
pub mod keys;
pub mod lab;
pub mod lxc_fs;
pub mod lxc_updates;
pub mod mcp_arr;
pub mod plugins;
pub mod fsutil;
pub mod procs;
pub mod pve;
pub mod remote;
pub mod server;
pub mod shred;
pub mod startup;
pub mod sys;
pub mod toolkit;
pub mod updater;
pub mod util;
pub mod vault;
pub mod web;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
