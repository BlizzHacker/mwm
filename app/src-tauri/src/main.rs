// MDW - Move Digital Weight desktop app: a thin Tauri shell over mdw-core.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use mdw_core::{apps, cleaner, disk, drivers, dupes, procs, shred, startup, sys, updater};
use serde::Serialize;

type Res<T> = Result<T, String>;

/// Run blocking engine work off the UI thread.
async fn bg<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn system_info() -> Res<sys::SystemInfo> {
    bg(sys::info).await
}

#[tauri::command]
async fn cleaner_scan() -> Res<Vec<cleaner::ScanItem>> {
    bg(cleaner::scan_all).await
}

#[tauri::command]
async fn cleaner_clean(ids: Vec<String>) -> Res<Vec<cleaner::CleanResult>> {
    bg(move || cleaner::clean(&ids)).await
}

#[tauri::command]
async fn apps_list(store: bool) -> Res<Vec<apps::App>> {
    bg(move || apps::list(store)).await
}

#[tauri::command]
async fn app_uninstall(app: apps::App, quiet: bool) -> Res<apps::Outcome> {
    bg(move || apps::uninstall(&app, quiet)).await
}

#[tauri::command]
async fn app_leftovers(app: apps::App) -> Res<Vec<apps::Leftover>> {
    bg(move || apps::leftovers(&app)).await
}

#[tauri::command]
async fn leftovers_remove(items: Vec<apps::Leftover>) -> Res<apps::RemoveReport> {
    bg(move || apps::remove_leftovers(&items)).await
}

#[tauri::command]
async fn startup_list() -> Res<Vec<startup::StartupItem>> {
    bg(startup::list).await
}

#[tauri::command]
async fn startup_set(id: String, enabled: bool) -> Res<startup::Outcome> {
    bg(move || startup::set_enabled(&id, enabled)).await
}

#[tauri::command]
async fn service_mode(name: String, mode: String) -> Res<startup::Outcome> {
    bg(move || startup::set_service_mode(&name, &mode)).await
}

#[tauri::command]
async fn updates_list() -> Res<Vec<updater::Update>> {
    bg(updater::list).await?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_apply(id: String) -> Res<updater::Outcome> {
    bg(move || updater::apply(&id)).await
}

#[tauri::command]
async fn dupes_find(roots: Vec<String>, min_size: u64) -> Res<Vec<dupes::DupGroup>> {
    bg(move || dupes::find(&roots.iter().map(PathBuf::from).collect::<Vec<_>>(), min_size, false)).await
}

#[derive(Serialize)]
struct Removed {
    removed: Vec<String>,
    failed: Vec<(String, String)>,
}

#[tauri::command]
async fn dupes_remove(paths: Vec<String>) -> Res<Removed> {
    bg(move || {
        let (removed, failed) = dupes::remove(&paths);
        Removed { removed, failed }
    })
    .await
}

#[tauri::command]
async fn disk_analyze(root: String) -> Res<disk::Report> {
    bg(move || disk::analyze(&PathBuf::from(root), 60)).await
}

#[tauri::command]
async fn shred_paths(paths: Vec<String>, passes: u32) -> Res<shred::Report> {
    bg(move || shred::shred(&paths, passes)).await
}

#[tauri::command]
async fn wipe_free(dir: String) -> Res<String> {
    bg(move || shred::wipe_free_space(&dir)).await?.map_err(|e| e.to_string())
}

#[tauri::command]
async fn drivers_list() -> Res<Vec<drivers::Driver>> {
    bg(drivers::list).await
}

#[tauri::command]
async fn procs_list() -> Res<Vec<procs::Proc>> {
    bg(procs::list).await
}

#[tauri::command]
async fn procs_kill(pids: Vec<u32>) -> Res<usize> {
    bg(move || procs::kill(&pids)).await
}

#[tauri::command]
fn reveal(path: String) -> Res<()> {
    sys::reveal(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn launch_tool(tool: String) -> Res<()> {
    sys::launch_tool(&tool).map_err(|e| e.to_string())
}

#[tauri::command]
fn relaunch_admin(app: tauri::AppHandle) -> Res<()> {
    sys::relaunch_elevated("").map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            system_info,
            cleaner_scan,
            cleaner_clean,
            apps_list,
            app_uninstall,
            app_leftovers,
            leftovers_remove,
            startup_list,
            startup_set,
            service_mode,
            updates_list,
            update_apply,
            dupes_find,
            dupes_remove,
            disk_analyze,
            shred_paths,
            wipe_free,
            drivers_list,
            procs_list,
            procs_kill,
            reveal,
            launch_tool,
            relaunch_admin
        ])
        .run(tauri::generate_context!())
        .expect("MDW failed to start");
}
