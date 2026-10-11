// MWM - Move Weight Manager desktop app: a thin Tauri shell over mwm-core.
// Every UI call goes through `api` -> mwm_core::api::call, the same table
// `mwm serve` uses, so desktop and web behave identically.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde_json::Value;

#[tauri::command]
async fn api(cmd: String, args: Option<Value>) -> Result<Value, String> {
    let args = args.unwrap_or(Value::Null);
    tauri::async_runtime::spawn_blocking(move || mwm_core::api::call(&cmd, &args))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn relaunch_admin(app: tauri::AppHandle) -> Result<(), String> {
    mwm_core::sys::relaunch_elevated("").map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

fn main() {
    // Headless modes for Task Scheduler: no window, do the job, exit.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--guard") {
        // Move Weight guard: move idle folders to the server, clean safe caches.
        mwm_core::offload::guard_run();
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--offload-setup") {
        // Import a Move Weight policy file (setup scripts), install the guard, exit.
        let ok = args
            .get(i + 1)
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<mwm_core::offload::Policy>(&t).ok())
            .map(|p| mwm_core::offload::set_policy(&p).is_ok())
            .unwrap_or(false);
        std::process::exit(if ok { 0 } else { 1 });
    }
    // Deep link: `MoveWeightManager.exe "C:\\some\\folder"` opens Commander there.
    if let Some(dir) = args.iter().skip(1).find(|a| std::path::Path::new(a).is_dir()) {
        mwm_core::sys::set_pending_dir(dir.clone());
    }
    // Remote access (this PC's own MWM server) comes back on if it was left on.
    std::thread::spawn(mwm_core::web::ra_autostart);
    // A newer MWM refreshes the Move Weight guard copy and its scheduled task.
    std::thread::spawn(mwm_core::offload::ensure_guard);
    tauri::Builder::default()
        // A second launch (Explorer takeover!) forwards its folder to this
        // instance and focuses the window instead of opening a second app.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(dir) = argv.iter().skip(1).find(|a| std::path::Path::new(a).is_dir()) {
                mwm_core::sys::set_pending_dir(dir.clone());
            }
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![api, relaunch_admin])
        .run(tauri::generate_context!())
        .expect("MWM failed to start");
}
