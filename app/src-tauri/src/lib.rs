// MWM - Move Weight Manager: the app library. A thin Tauri shell over
// mwm-core. Every UI call goes through `api` -> mwm_core::api::call, the same
// command table `mwm serve` uses, so desktop, server-web and mobile behave
// identically. `run()` is the entry point for both the desktop binary and the
// Android/iOS mobile shells (tauri::mobile_entry_point).

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

/// Desktop-only startup conveniences (CLI deep-link, guard, remote-access
/// autostart). Skipped on mobile, where there is no filesystem server role.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn desktop_preflight() {
    let args: Vec<String> = std::env::args().collect();
    // Headless modes for Task Scheduler: no window, do the job, exit.
    if args.iter().any(|a| a == "--guard") {
        mwm_core::offload::guard_run();
        std::process::exit(0);
    }
    if let Some(i) = args.iter().position(|a| a == "--offload-setup") {
        let ok = args
            .get(i + 1)
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<mwm_core::offload::Policy>(&t).ok())
            .map(|p| mwm_core::offload::set_policy(&p).is_ok())
            .unwrap_or(false);
        std::process::exit(if ok { 0 } else { 1 });
    }
    // Deep link: `MoveWeightManager.exe "C:\some\folder"` opens Commander there.
    if let Some(dir) = args.iter().skip(1).find(|a| std::path::Path::new(a).is_dir()) {
        mwm_core::sys::set_pending_dir(dir.clone());
    }
    // Remote access (this PC's own MWM server) comes back on if it was left on.
    std::thread::spawn(mwm_core::web::ra_autostart);
    // A newer MWM refreshes the Move Weight guard copy and its scheduled task.
    std::thread::spawn(mwm_core::offload::ensure_guard);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    desktop_preflight();

    let builder = tauri::Builder::default();

    // single-instance only exists on desktop.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
        // A second launch (Explorer takeover!) forwards its folder to this
        // instance and focuses the window instead of opening a second app.
        if let Some(dir) = argv.iter().skip(1).find(|a| std::path::Path::new(a).is_dir()) {
            mwm_core::sys::set_pending_dir(dir.clone());
        }
        use tauri::Manager;
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![api, relaunch_admin])
        .run(tauri::generate_context!())
        .expect("MWM failed to start");
}
