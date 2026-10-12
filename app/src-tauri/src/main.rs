// MWM - Move Weight Manager desktop binary. All logic lives in lib.rs so the
// same crate builds as a desktop exe and as an Android/iOS mobile shell.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mwm_app::run();
}
