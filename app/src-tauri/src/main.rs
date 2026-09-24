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
    // Remote access (this PC's own MWM server) comes back on if it was left on.
    std::thread::spawn(mwm_core::web::ra_autostart);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![api, relaunch_admin])
        .run(tauri::generate_context!())
        .expect("MWM failed to start");
}
